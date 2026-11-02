# termana 代码评审报告（2026-11-02）

> 只读评审：未修改任何源码、未执行 git commit/push。除本报告外未写入任何文件
> （`npm run build` 生成的 `dist/` 为 .gitignore 忽略的构建产物，构建后 `git status --porcelain` 输出为 0 行，工作区保持干净）。

## 1. 仓库定位与规模概览

- **定位**：本地优先的 Tauri v2 桌面应用「终端项目启动器」——项目卡片绑定 CLI coding agent，一键打开系统终端（macOS Terminal.app / Windows PowerShell）并启动 agent；内置 AGENTS.md 上下文编辑器；带更新检查与公告功能。
- **技术栈**：Rust（`src-tauri/`，Tauri 命令后端）+ Vanilla TypeScript + Vite（前端 webview）+ TOML 配置持久化。
- **规模**：git 跟踪文件 57 个；核心代码约 4,519 行——
  - Rust 1,946 行（`commands.rs` 1,038、`config.rs` 287、`update.rs` 292、`adapters/agent.rs` 170、`adapters/terminal.rs` 115、`lib.rs`/`main.rs`/`mod.rs` 44）
  - 前端 `src/main.ts` 1,315 行、`src/styles.css` 1,137 行、`index.html` 121 行
- **测试**：Rust 32 个单元测试（命令层上下文状态机、配置解析、版本比较、asset 选择、可执行名提取等）；前端无测试运行器。
- **CI**：`.github/workflows/ci.yml`（macos-latest 上 `npm run check` + `npm test`）、`.github/workflows/release.yml`（三平台构建矩阵：macOS aarch64/x86_64、Windows x86_64，含版本解析与签名发布）。

## 2. 结论摘要

| 级别 | 数量 |
|---|---|
| P0 | 0 |
| P1 | 2 |
| P2 | 5 |
| P3 | 7 |

**最关键的 3 条**：

1. **P1 — macOS 启动链路存在单引号 shell 注入/启动失败**：`escape_applescript` 只转义反斜杠与双引号，而窗口标题（含用户项目名）被拼进 shell 单引号字符串，含 `'` 的名字会导致启动失败，构造的目录名可在用户终端执行任意命令（`src-tauri/src/adapters/terminal.rs:17-19, 42-44`）。
2. **P1 — 自动更新最后一步 `relaunch()` 必失败**：JS 侧依赖 `@tauri-apps/plugin-process` 并调用 `relaunch()`，但 Rust 侧未引入/注册 `tauri-plugin-process`、capabilities 未授权，更新安装成功后反而弹出「自动更新失败」（`src/main.ts:4, 718-733` vs `src-tauri/Cargo.toml:15-25`、`src-tauri/src/lib.rs:8-10`、`src-tauri/capabilities/default.json`）。
3. **P2 — 远程公告 Markdown 未经消毒渲染**：公告内容（来自 GitHub raw 的远程数据）用默认 `marked.parse` 直接注入 DOM，绕过了上下文预览使用的消毒 renderer，CSP 虽挡内联脚本但挡不住 `<style>` 覆盖与任意标记注入（`src/main.ts:567, 609` vs `105-108, 404`）。

## 3. 逐条发现

### 3.1 架构与模块边界

整体分层清晰：Tauri 命令层（`commands.rs`）/ 配置层（`config.rs`）/ 适配层（`adapters/`）/ 更新层（`update.rs`），前端单 island 通过 `invoke` 调用。架构本身无重大问题，以下按维度列具体缺陷。

### 3.2 正确性与潜在 bug

**[P1-1] macOS 终端启动：标题单引号未转义，可致 shell 注入与合法名字启动失败**

- 位置：`src-tauri/src/adapters/terminal.rs:17-19`（`escape_applescript` 仅替换 `\` 与 `"`）、`terminal.rs:42-44`（标题拼入 `printf '{title_part}'` 单引号 shell 字符串）；标题来源 `src-tauri/src/commands.rs:71`（`format!("{} — {}", project.name, agent_name)`）；项目名可由文件夹名自动填充（`src/main.ts:765-769`）。
- 证据链：AppleScript 字符串内 `\\` → `\`，shell 实际收到 `printf '\033]0;<标题>\007'; cd '<dir>'`。标题含 `'`（如 `Bob's App`，或恶意目录名 `x'; touch /tmp/poc; echo '`）时单引号提前闭合，轻则 shell 语法错误启动失败，重则在用户终端执行注入命令。
- 影响：数据完整性外的命令执行面；触发条件低（用户添加含引号项目/目录）。
- 修复建议：对 title 先做 POSIX 单引号转义（`'` → `'\''`）再经 `escape_applescript`，或改用 AppleScript `quoted form of` 传递整个 printf 参数；为 `MacTerminal::launch` 生成脚本增加含引号名字的单元测试（目前 adapters 零测试，见 3.6）。

**[P1-2] 自动更新流程断链：`relaunch()` 所需插件未注册，安装成功却报失败**

- 位置：调用方 `src/main.ts:4`（`import { relaunch } from "@tauri-apps/plugin-process"`）、`src/main.ts:723-724`（`await update.downloadAndInstall(); await relaunch();`）。
- 证据：`src-tauri/Cargo.toml:15-25` 依赖中无 `tauri-plugin-process`；`src-tauri/src/lib.rs:8-10` 只注册 dialog 与 updater 插件；`src-tauri/capabilities/default.json:6-10` 权限仅 `core:default / dialog:default / updater:default`。Tauri v2 中未注册插件的 `plugin:process|restart` 命令不存在，`relaunch()` 必抛错。
- 影响：`downloadAndInstall()` 成功后 `relaunch()` 抛错被 `catch (updaterErr)`（`src/main.ts:725-733`）捕获，用户看到「自动更新失败，请手动下载」——实际新版本已安装，重启即生效。功能可用性与错误提示双重错误。
- 修复建议：`Cargo.toml` 加 `tauri-plugin-process = "2"`，`lib.rs` 加 `.plugin(tauri_plugin_process::init())`，capabilities 加 `process:default`（或最小化 `process:allow-restart`）；或改为提示用户手动重启。

**[P2-2] 失败启动也被计入启动统计**

- 位置：`src-tauri/src/commands.rs:150-158` `launch_project`：先 `p.last_launched = Some(now()); p.launch_count += 1;` 并 `config::save(&cfg)`，之后才调用 `resolve_and_launch`；而后者在 agent 未安装时返回错误（`commands.rs:87-92`）。
- 影响：启动失败的项目 `launchCount` 与「上次启动」徽章照常累计，排序「最近」与 UI 徽标失真。
- 修复建议：先做安装检测、启动成功后再更新统计（或失败时回滚计数）。

**[P2-3] 配置读写无并发保护，读-改-写可丢更新**

- 位置：`src-tauri/src/commands.rs` 各写命令均为 `config::load()` → 修改 → `config::save(&cfg)`（如 118-140、162-178、640-659），无任何互斥；Tauri 命令可并发执行。
- 证据：`config::save` 的原子替换（`config.rs:154-160` tempfile + persist）只防半写文件，不防两次并发「读-改-写」互相覆盖（如拖拽排序 `reorder_projects` 与 `add_project` 同时发生时后写者丢前写者改动）。
- 影响：高频 UI 操作下配置条目可能丢失（低频但真实）。
- 修复建议：进程内全局 `Mutex`（或 `RwLock` 读多写少）包裹 load→save 临界区。

**[P3-1] `launch_group` 不更新启动统计，与单启逻辑不一致**

- 位置：`commands.rs:719-741` 直接 `resolve_and_launch`，未更新 `last_launched/launch_count`；对比 `launch_project`（150-158）。
- 影响：仅通过组启动的项目永远显示 0 次启动。
- 修复建议：统一两条路径的统计更新（结合 P2-2 一并改）。

**[P3-6] `add_project` 后端不校验路径存在**

- 位置：`commands.rs:118-124` 仅校验非空；路径存在性只在前端 `path_exists` 把关（`main.ts:467-491`）。
- 影响：绕过 UI 直接 IPC 可注册不存在的路径，启动时才暴露。本地单用户应用，风险低。
- 修复建议：后端 `add_project` 复用 `Path::is_dir()` 校验。

### 3.3 安全

**[P2-4] 远程公告 Markdown 未消毒即注入 DOM**

- 位置：`src/main.ts:567`（进行中公告）与 `src/main.ts:609`（历史公告）：`await marked.parse(a.content)` 使用默认 renderer；对比上下文预览 `src/main.ts:105-108`（自定义 `previewRenderer`：html/image 全部转义、link 不输出 href）与 `src/main.ts:404`。
- 数据来源：远程 `https://raw.githubusercontent.com/bayernjf/termana/main/announcements.json`（`src-tauri/src/update.rs:4-5, 183-197`）。
- 缓解因素：CSP（`src-tauri/tauri.conf.json:22`）`default-src 'self'` 使 `script-src` 回落到 `'self'`，内联脚本与事件处理器被阻断；`img-src` 不含远程源。但 `style-src 'self' 'unsafe-inline'` 允许 `<style>` 标签与 style 属性，钓鱼式 UI 覆盖、假按钮外链（`<a href="https://…">`）仍可注入。
- 与文档不一致：`docs/CHANGELOG.md` v1.1 声称「Markdown 预览禁用可执行 HTML、链接导航与远程图片」——该承诺只覆盖了上下文预览，未覆盖公告。
- 影响：需要仓库 main 分支写权限（账号失陷/恶意 PR 合入）作为前置，属供应链纵深防御缺口。
- 修复建议：公告渲染复用 `previewRenderer`（或在 renderer 上增加显式白名单），并修正 CHANGELOG 表述。

**[P3-2] 安装检测把可执行名未加引号拼进 shell，且输出对齐可被 profile 输出干扰**

- 位置：`src-tauri/src/adapters/agent.rs:71-79`：`command -v {c}` 直接拼接首个 token。含反引号或 `$(…)` 的自定义 agent 命令会被 login shell 求值（仅限用户自己写入 config.toml 的内容，自我注入）。
- 同函数 `agent.rs:115-130` `parse_flags` 靠按行过滤 `"1"/"0"` 对齐结果；用户 `.zshrc`/profile 若打印恰好为 `1` 或 `0` 的行会使结果错位（检测以 `-ilc` 交互登录 shell 执行，`agent.rs:80-82`）。
- 修复建议：对可执行名单引号转义；检测结果用带索引前缀的格式（如 `printf '%d:1\n' i`）按索引归位。

**[P3-5] 配置目录解析失败时回退到 CWD 相对路径**

- 位置：`src-tauri/src/config.rs:97-100`：`dirs::config_dir().unwrap_or_else(|| PathBuf::from("."))` → 配置写入 `./termana/config.toml`，位置随启动时工作目录漂移。
- 修复建议：失败时返回错误或使用 home 目录回退。

其他安全面核查（无发现）：
- 源码中未发现硬编码密钥（`grep -ri "api_key|secret|password|token" src src-tauri` 仅命中代码符号与 target 构建产物）；`tauri.conf.json:43` 的 updater `pubkey` 为公钥，可安全提交（release.yml 注释亦说明）。
- 前端所有用户可控字符串（项目名/路径/agent 名等）经 `escapeHtml` 后 innerHTML（`main.ts:99-103, 222-242, 251-273, 300-318`），公告见 P2-4。
- capabilities 最小化，无 shell/fs 插件暴露；CSP 已启用。

### 3.4 数据模型与迁移一致性

- 旧字段 `Project.context` 以 `#[serde(default, rename = "context", skip_serializing_if = "Option::is_none")]` 平滑迁移为 `legacy_context`（`src-tauri/src/config.rs:14-15`），并有测试覆盖：pre-v1 配置解析、legacy key 清除后不再出现、未知 key 前向兼容（`config.rs:176-216`）。
- 上下文文件并发冲突用 FNV-1a revision 乐观锁，保存前校验、stale 拒绝（`commands.rs:237-245, 419-464`），测试覆盖 stale 拒绝与 symlink 不写穿（`commands.rs:983-1037`）。设计自洽。
- **[P3-3]** `MIGRATION_CONVENTION.md` 与 `tools/check-migrations.sh` 面向数据库 migration（Supabase/Postgres），但本仓无任何数据库与 migration 目录，疑似从其他项目拷贝，属于文档体系噪音。建议移除或注明适用条件。

### 3.5 错误处理与失败模式

- 优点：配置损坏时 `load_with_status` 上报 warning、`save` 拒绝覆盖以防数据销毁（`config.rs:105-162`），UI 有 banner 提示（`main.ts:510-519`）；写文件全部走同目录 tempfile + fsync + persist（`commands.rs:319-347`、`config.rs:154-160`）；网络请求带超时（`update.rs:128-132, 177-181`）。
- 缺陷：P2-2（失败启动仍计数）、P1-2（更新失败误报）。`launch_group` 聚合各项目错误并整体返回（`commands.rs:727-740`），行为合理。

### 3.6 测试覆盖与验证门真实性

- Rust 32 个单元测试均为真实断言（抽查 `commands.rs:801-1037`、`config.rs:164-287`、`update.rs:199-292`、`agent.rs:132-170`，含负路径与回归注释），非摆设。
- CI（`.github/workflows/ci.yml`）真实执行 `npm run check` 与 `npm test`，macos-latest，无 `|| true` 类放水。
- 缺口（handoff.md 自述并核实）：
  - **[P2-5 的一部分]** 前端 1,315 行 `main.ts` 无任何测试运行器，唯一门禁是 `tsc --noEmit`。
  - adapters（terminal/agent 的 IO 层）基本无测试——P1-1 正位于无测试的 `terminal.rs`。handoff 记录行覆盖约 52%（`commands.rs` 52%、`config.rs` 71%、`update.rs` 61%），覆盖率未设门禁。

### 3.7 依赖与构建健康

- `npm ci` 可复现（`package-lock.json` 在仓）；`marked@18.0.7`（实装确认）。Vite 6.4.3 构建产物 gzip 22 KB JS，健康。
- Rust 依赖均为成熟 crate（tauri 2、serde、toml、tempfile、reqwest blocking、tokio）。注意 `tokio` 以 `full` feature 引入但代码中未直接使用（grep `src-tauri/src` 无 tokio 引用），属于可裁剪的编译开销（P3，不计入总数单列，供参考）。
- 未做 `cargo audit`/`npm audit` 在线漏洞比对（只读约束下未联网查询，标记「未确认」）。

### 3.8 性能热点

- **[P3-4]** 项目刷新为 N+1 模式：`main.ts:377-385` 对每个项目单独 `invoke("context_status")`，而后端每次调用都重新 `config::load()`（`commands.rs:565-573` 经 `commands.rs:113-115` 同款模式），每次刷新产生 N 次 IPC + N 次配置读解析 + 2N 次文件读。项目数几十内无感，属低优先优化（后端批量接口或 config 缓存）。
- macOS 启动依赖 AppleScript `delay 0.5` 固定等待（`terminal.rs:44`），重 shell 配置机器上存在时序脆弱性（低风险，未实测，标「未确认」）。

### 3.9 文档与实现一致性

- **[P2-5]** README 提供 Linux 安装指引（`README.md:67-69`：`.deb` / `.AppImage` / `.app.tar.gz`），但 `src-tauri/src/adapters/terminal.rs:111-114` 对非 macOS/Windows 目标 `compile_error!`，且 `release.yml` 构建矩阵仅 macOS + Windows——Linux 既无法编译也无发布物。建议删除 README Linux 段或实现 Linux 适配。
- **[P3-6]** `update.rs::is_newer` 对含预发布后缀的版本静默丢段（如 `"0.1.0-rc1"` → `[0,1,0]`），测试已如实记录该陷阱（`update.rs:241-248`）。当前 `/releases/latest` API 不返回 prerelease，实际不可达，低风险。
- 其余文档（`docs/technical-design.md` 架构图、`handoff.md` 状态、AGENTS.md 命令表）与实现核对一致；handoff.md 自述「32 Rust tests」与实测一致。
- CHANGELOG「禁用可执行 HTML」表述与公告实现不一致（见 P2-4）。

## 4. 验证门实际执行结果

| 命令 | 结果 | 输出摘要 |
|---|---|---|
| `npm run check`（tsc --noEmit） | 通过 | 无错误输出（exit 0） |
| `npm test`（cargo test） | 通过 | `test result: ok. 32 passed; 0 failed`（lib 测试 0.13s；main.rs 与 doc-tests 各 0 个） |
| `npm run build`（tsc && vite build） | 通过 | vite 6.4.3 构建成功：index.html 5.86 kB、CSS 16.09 kB、JS 74.00 kB（gzip 21.92 kB），667ms |
| 构建后 `git status --porcelain` | 干净 | 0 行（dist 被 .gitignore 覆盖） |
| `npm run tauri:dev` / `tauri:build` | 未执行 | 常驻进程需占用端口 / 打包耗时长，只读评审约束下不必要；P1-2 的结论来自静态证据链（依赖清单 + 插件注册 + capabilities 三处一致缺失），未做真机点击验证，标记「未实测」 |

依赖已预装（node_modules 与 src-tauri/target 均存在），未安装任何新依赖，未启动常驻服务，未 kill 任何进程。

## 5. 修复优先级建议

1. **P1-2（自动更新断链）**：三处小改动（Cargo.toml + lib.rs + capabilities）即可修复，直接恢复核心卖点功能，建议最先修。
2. **P1-1（标题引号注入）**：单行转义修复 + 为 `terminal.rs` 补首个单元测试；顺带解决含撇号合法名字的启动失败。
3. **P2-4（公告消毒）**：一行切换为 `previewRenderer`，消除供应链注入面。
4. **P2-2 + P3-1（启动统计一致性）**：调整统计写入时机，统一单启/组启路径。
5. **P2-3（配置并发）**：加全局互斥，防丢更新。
6. **P2-5（README Linux 段）**：删文档或补实现，消除用户误导。
7. P3 批次（N+1 IPC、agent 检测转义与对齐、MIGRATION 文档清理、config 路径回退、后端路径校验、tokio 裁剪）按迭代容量择机处理。
