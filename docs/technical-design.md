# termana - 技术方案

## 1. 架构总览
本地优先的桌面应用：
- **前端**：vanilla TS + Vite，渲染在 Tauri webview 里（项目 / 组 / agent 面板 + 上下文编辑器）。
- **后端**：Rust（Tauri 后端），负责配置持久化、终端启动、路径校验、安装检测、上下文文件读写。
- **终端**：不嵌入，drive 用户系统自带的终端（macOS Terminal.app / Windows PowerShell）。

```
┌─────────────────────────────────┐
│  Tauri Webview (vanilla TS UI)  │  Projects / Groups / Agents / Context editor
└──────────────┬──────────────────┘
               │ invoke (Tauri commands) + dialog plugin
┌──────────────▼──────────────────┐
│  Rust 后端                       │
│  ├─ config.rs    Project/Agent/Group + TOML 持久化 + builtin_agents()
│  ├─ commands.rs  project/agent/group CRUD · launch · path_exists ·
│  │               read_context/save_context/context_status（v1 编辑器）
│  └─ adapters/                   │
│     ├─ agent.rs    installed_status() 安装检测
│     └─ terminal.rs TerminalAdapter: macOS / Windows
└──────────────┬──────────────────┘
               │ spawn
        ┌──────▼──────┐
        │ 系统终端      │ macOS Terminal.app / Windows PowerShell
        └─────────────┘
```

## 2. 技术栈选型与理由
| 决策 | 选择 | 理由 |
|---|---|---|
| 桌面框架 | Tauri v2（非 Electron） | 后端要干系统活（spawn 进程、读写项目文件），Rust 合身；体积小、启动快；权限白名单更安全。不嵌终端使 Electron 的 xterm.js 优势失效。 |
| 终端策略 | drive 系统终端（非嵌入） | 不造终端模拟器；尊重终端爱好者已有的 tmux / 终端环境；工程量从一年降到几周。 |
| 前端 | vanilla TS | 面板 UI 简单，无需框架；bundle 极小。UI 复杂化后可换 Svelte。 |
| 弹窗 | tauri-plugin-dialog | webview 屏蔽同步 `confirm()`/`alert()`，确认 / 错误改用异步 `confirm`/`message`；文件夹选择用 `open`。`dialog:default` 权限。 |
| Markdown 预览 | marked | 上下文编辑器的"预览"tab 渲染 AGENTS.md（h/p/list/code/blockquote 等）。 |
| 上下文归一化 | `AGENTS.md` + `@AGENTS.md` 指针（生态标准） | 不自造多格式 emit 引擎。AGENTS.md 是正本，CLAUDE.md 是纯文本 `@AGENTS.md` 指针（非 symlink，Windows/Git 安全）。 |
| 配置格式 | TOML | 终端工具惯例，人可读可手改。 |
| 部署 | 本地优先，无后端服务 | 核心动作是 spawn 本地进程 + 读写本地文件，浏览器沙箱做不到；离线、无账号是卖点。 |

> dev server 端口为 **1442**（非 Tauri 默认 1420，因开发机 1420 被占），见 [vite.config.ts](../vite.config.ts) 与 [tauri.conf.json](../src-tauri/tauri.conf.json) `devUrl`。

## 3. 模块结构
```
src-tauri/src/
├── lib.rs              应用入口，注册命令 + dialog 插件
├── config.rs           Project / Agent / Group 模型 + TOML 读写 + builtin_agents()（agents.toml）+ slugify
├── commands.rs         projects: list/add/remove/launch · agents: list/add/update/remove ·
│                       groups: list/add/update/remove/launch · path_exists ·
│                       context: read_context/save_context/context_status（v1）
└── adapters/
    ├── mod.rs
    ├── agent.rs        installed_status() 安装检测（login shell / Get-Command）
    └── terminal.rs     TerminalAdapter trait + MacTerminal / WindowsPowerShell
agents.toml             built-in 预设（纯 key-value：`"名称" = "命令"`，include_str! 编译期嵌入）
src/
├── main.ts             面板 UI（项目 / 组 / agent）+ 上下文编辑器 + 事件 + 路径校验 + 异步弹窗
└── styles.css          暗色主题
```

## 4. 数据模型
config 路径：`~/Library/Application Support/termana/config.toml`（macOS）/ `%APPDATA%\termana\config.toml`（Windows）

```toml
[[projects]]
id = "my-app"
name = "My App"
path = "/Users/me/projects/my-app"
agent = "claude"

[[groups]]
id = "backend"
name = "Backend"
project_ids = ["my-app", "api-server"]

[[agents]]          # 仅用户自定义 agent；built-in 来自 agents.toml
id = "gemini"
name = "Gemini"
command = "gemini"
```

> **v1 迁移**：`Project.context` 不再是正本，仅以 `legacy_context` 兼容读取，成功写入 AGENTS.md 后清除。

built-in 预设在项目根 [agents.toml](../agents.toml)：
```toml
"Claude Code" = "claude"
"Codex" = "codex"
"Aider" = "aider"
"OpenCode" = "opencode"
```

未来配置绑定深化（project，非 v1 上下文范畴）：
```toml
model = "claude-sonnet-5"
mcp_servers = ["filesystem", "github"]
permissions = ["read", "write"]
```

## 5. 关键流程

### 启动项目（launch_project）
1. 前端 `invoke("launch_project", { id })`
2. `resolve_and_launch(cfg, id)`：找 project，解析命令（built-in agent > 自定义 agent > 原始 id）
3. `default_terminal()` 按 OS 选适配器，spawn 终端 cd + 跑命令
   - macOS：`osascript` -> `tell application "Terminal" to do script "cd '<path>' && <cmd>"`
   - Windows：`powershell -NoExit -Command "Set-Location '<path>'; <cmd>"`（`CREATE_NEW_CONSOLE` 开新窗口，加载 profile）

> **v1 移除**：启动时自动 sync。启动只管拉终端，不碰上下文文件（文件即源，无需 sync）。

### 启动组（launch_group）
1. `invoke("launch_group", { id })` -> 找 group
2. 循环 `group.project_ids`，逐个 `resolve_and_launch` -> 每个项目各开一个终端

### 添加项目（含校验）
1. 📁 Browse -> `open({ directory: true })` 选文件夹，路径 + 项目名（取文件夹名）自动填充
2. 路径失焦 / 回车 / 输入停顿 300ms -> `path_exists(path)`（`Path::is_dir()`）校验；不存在则红框 + 禁用 Add
3. `add_project(name, path, agent)` 存 config

### 卡片交互
- `Launch ▸`（hover 提示，可点）-> 直接启动
- 点卡片主体 -> `confirm("Launch ...?")` 二次确认后启动
- `✕` -> `confirm("Remove ...?")` 确认删除
- 组卡片同款样式，`Launch all ▸` 直接启动全组

### 安装检测（installed_status）
- 一次性在用户实际用的 shell 里查所有 agent 命令：unix `$SHELL -ilc` 跑 `command -v`，windows `powershell -Command` 跑 `Get-Command -CommandType Application`
- 只认外部命令（绝对路径 / Application），shell builtin（如 `continue`）不算
- 触发时机：app 启动、agent 增删改后、点 ↻ Refresh

### 弹窗
- 确认 / 错误用 `@tauri-apps/plugin-dialog` 的 `confirm` / `message`（异步），不用同步 `confirm()`/`alert()`（被 webview 屏蔽）

### 上下文编辑（v1）

**文件**：`AGENTS.md`（正本）、`CLAUDE.md`（指针）。常量 `CONTEXT_FILES` / 文件名集中在 commands.rs。

**加载（`read_context`）**--优先级 AGENTS.md 有内容 > CLAUDE.md 有内容 > 空：

```
agents   = read(project/AGENTS.md)   # None if missing/empty
claude   = read(project/CLAUDE.md)   # None if missing/empty
# "已链接"放宽为含 "@AGENTS.md"：纯指针 / import 行 / 带 Claude 备注 都算，
# 不误判为独立内容而清掉备注（egg/coreos 的成熟写法）。
claude_is_linked  = claude 非空 且 含 "@AGENTS.md"
agents_has_block  = agents 含 "<!-- termana-merge-block"   # 已有遗留合并块

# 情形 6：两者都有真实内容、claude 非链接、不一致、且 AGENTS.md 还没合并块
# （已有块就不重复追加，避免重复拼接--见"失败处理"）
if agents.is_some() and claude.is_some() and not claude_is_linked
   and agents != claude and not agents_has_block:
    return {
        content: agents + 合并块(claude),   # 见下
        source: "merge"
    }
# AGENTS.md 有内容（情形 4、5；以及 6 中两者一致的情况）
if agents.is_some():
    return { content: agents, source: "agents" }
# AGENTS.md 空、CLAUDE.md 有真实内容（情形 3：提升）
if claude.is_some() and not claude_is_linked:
    return { content: claude, source: "claude" }
# 其余（情形 1、2）：空
return { content: "", source: "empty" }
```

合并块（情形 6 拼接）：
```
<AGENTS.md 原内容>

---

<!-- termana-merge-block: from CLAUDE.md -->

> ⚠️ 以下来自 CLAUDE.md，与上文不一致，需合并。规则：去重、解决冲突、保留所有独有信息，合并完后删除本块（含本说明）。
>
> AI agent：若被要求整理本文档，请按上述规则把下方内容并入正文，再删除本块。

<CLAUDE.md 内容>
```

**保存（`save_context`）**--编辑器内容永远写 AGENTS.md，保存前先校验打开编辑器时取得的三个 revision；任一来源变化则拒绝覆盖：

```
# 1. 先写正本（canonical）。失败 -> 整个保存失败，CLAUDE.md 不动，无副作用。
write content -> project/AGENTS.md      on error: return Err("无法写入 AGENTS.md: {reason}")
has_block = content 含 "<!-- termana-merge-block"

# 2. CLAUDE.md 若是 symlink，绝不写--fs::write 对 symlink 是写穿到目标，
#    next.js 的 CLAUDE.md->AGENTS.md 软链写穿会把 AGENTS.md 变成自指 @AGENTS.md，毁正本。
if symlink_metadata(CLAUDE.md).is_symlink():
    return Ok({ claudeAction: "symlink-skip", claudeError: None, hasMergeBlock: has_block })

# 3. 非 symlink，按内容分类处理 CLAUDE.md。失败 -> 部分成功（内容已安全落在 AGENTS.md）。
claude = read(project/CLAUDE.md)        # 重读当前状态
match claude:
  缺失/空:            write "@AGENTS.md\n" -> CLAUDE.md;  action = "created"        # 情形 1、4
  已链接(含@AGENTS.md): 不动;                              action = "already-linked"  # 情形 5（及 2）
  独立内容 + 用户确认: write "@AGENTS.md\n" -> CLAUDE.md; action = "converted"
  独立内容 + 用户取消: 不动;                              action = "independent-kept"
on CLAUDE.md write error: action = "failed", claudeError = Some(reason)   # 部分成功

return Ok({ claudeAction: action, claudeError, hasMergeBlock: has_block })
```

**失败处理**：
- **顺序是 AGENTS.md 先、CLAUDE.md 后**。正本先落盘：AGENTS.md 失败 = 整个 `Err`、CLAUDE.md 未碰、无副作用、可重试；反过来先写指针再写正本，若正本失败会留下指向缺失/过时文件的指针，更糟。
- **AGENTS.md 写失败**（路径不存在 / 只读 / 磁盘满 / 权限）-> `Err(msg)`，前端弹"保存失败：{msg}"，无任何改动。
- **CLAUDE.md 写失败** -> `Ok` + `claudeAction="failed"` + `claudeError`。前端：成功提示"AGENTS.md 已保存" + 警告"CLAUDE.md 指针失败：{reason}，Claude Code 暂读不到，可重试"。内容已在 AGENTS.md，无丢失。
- **自愈**：部分成功后下次保存会重试 CLAUDE.md 指针（情形 3/6 的 CLAUDE.md 仍是独立内容 -> 再走 converted 路径）。情形 3 下 AGENTS.md 与 CLAUDE.md 内容已相同，转指针零丢失；情形 6 下 CLAUDE.md 内容已在 AGENTS.md 合并块里，转指针零丢失。
- **CLAUDE.md 是 symlink 不写**：`symlink_metadata` 检测；写穿会毁目标（next.js 软链写穿把 AGENTS.md 变自指指针）。判 `claudeAction="symlink-skip"`，前端提示"CLAUDE.md 是符号链接，termana 不改动"。
- **并发修改**：`read_context` 返回 AGENTS / CLAUDE / legacy revision；`save_context` 写入前校验，外部修改时拒绝覆盖并要求重新打开。
- **旧配置迁移**：`Project.context` 只作为 legacy source 保留。文件为空时直接提升；与文件不同时追加来源明确的合并块；成功保存后原子清除旧字段。
- **原子写入**：三个持久化目标均使用同目录临时文件、flush + 原子替换，单文件写失败不会留下半截内容。

**状态查询（`context_status`）**--供徽章 + 合并块提醒，刷新时调用：

```
return {
  agentsExists:      AGENTS.md 存在,
  agentsHasContent:  AGENTS.md 非空,
  claudeState:       "absent" | "empty" | "linked" | "independent" | "symlink",
  hasMergeBlock:     AGENTS.md 含 "<!-- termana-merge-block",
  divergent:         情形 6 条件（两者有内容、claude 非链接、不一致）
}
  # claudeState: linked=含 @AGENTS.md（纯指针/import/带备注）；symlink=是符号链接（termana 不写）
```

**合并块提醒**：保存后若 `hasMergeBlock=true`（用户没删合并块就存了），前端警告"合并块仍在 AGENTS.md，记得处理"。情形 6 一旦 CLAUDE.md 转成指针，下次打开即情形 5（AGENTS.md 里的遗留合并块原样显示，因 `agents_has_block` 守卫不再重复追加，哨兵触发提醒，用户继续合并）。

**命令签名**：

| 命令 | 入参 | 返回 | 说明 |
|---|---|---|---|
| `read_context` | `projectId` | `{ content, source, agentsRevision, claudeRevision, legacyRevision, requiresClaudeConversion, hasLegacyContext }` | 加载编辑器内容、迁移源和并发 token |
| `save_context` | `projectId, content, expected*Revision, convertClaude, migrateLegacy` | `{ claudeAction, claudeError, hasMergeBlock, legacyMigrated, legacyError }` | 原子写 AGENTS.md；按确认处理 CLAUDE.md；清理旧配置 |
| `context_status` | `projectId` | `{ agentsExists, agentsHasContent, claudeState: "absent"\|"empty"\|"linked"\|"independent"\|"symlink", hasMergeBlock, divergent }` | 徽章 + 合并块 + 不一致提醒 |

**移除的命令**：`get_context`、`set_context`、`sync_context`、`write_context_files`。

**前端改动**：
- ✎ 打开编辑器 -> `invoke("read_context")`（不再 `get_context`）。
- `ctx` 徽章：基于 `context_status.agentsHasContent`（不再 `Project.context`）；`hasMergeBlock=true` 显示"merge"提醒；`divergent=true` 显示"⚠ 不一致"徽章（打开编辑器前就提示去对齐）。
- Save 按钮 -> `invoke("save_context")`，按 `claudeAction` / `hasMergeBlock` 弹提示；`claudeAction="symlink-skip"` 提示"CLAUDE.md 是符号链接，termana 不改动"；`hasMergeBlock=true` 警告"合并块仍在，记得处理"。**移除 Sync 按钮**。
- Cancel 按钮：若有未保存改动（dirty）先 `confirm("放弃未保存的改动？")`，确认才关。
- 编辑器标签：`项目名 · agent -> AGENTS.md (+ CLAUDE.md 指针)`。

## 6. 适配器设计
终端与 agent 都走 adapter 模式，加新终端 / 新 agent = 加一个文件：
- **TerminalAdapter** trait：`launch(dir, command)`。已实现：`MacTerminal`、`WindowsPowerShell`。未来：iTerm2、Ghostty、Alacritty、Kitty。
- **Agent 列表**：built-in 预设从 `agents.toml` 加载（标记 built-in、不可改不可删）；自定义 agent 存 config、可增删改。`list_agents` 合并两者并做安装检测。
- **resolve_and_launch**：project -> agent command -> terminal，project 与 group 启动共用。
- **无 AgentAdapter / context emit**：`AGENTS.md` 是跨 agent 通用正本，无需 per-agent 派生。Claude Code 通过 `CLAUDE.md` 的 `@AGENTS.md` 指针读到同一份。
- **覆盖边界**：AGENTS.md + CLAUDE.md 只覆盖"读这两个文件之一"的 agent。built-in（Claude Code/Codex/Aider/OpenCode）全覆盖；自定义 agent 若读 `.cursorrules` / `copilot-instructions.md` / `CONVENTIONS.md` 等，termana 编辑 AGENTS.md 对它无效--已知边界，非 bug。

## 7. 跨平台注意
- macOS 分支 `#[cfg(target_os = "macos")]`，Windows 分支 `#[cfg(target_os = "windows")]`，按 OS 编译。
- **Windows 适配器在 macOS 上无法编译 / 测试**（cfg 跳过），需在 Windows 机器验证。
- 路径带空格：macOS 用 AppleScript `quoted form of`；Windows 由 `powershell -Command` + 单引号处理。
- **`@AGENTS.md` 指针是纯文本文件**（非 symlink），macOS / Windows / Git 均无特殊处理问题。next.js 的 symlink 方案在 Windows 上有 Developer Mode / Git checkout 把 symlink 变文本的问题，termana 不采用。

## 8. 已建 vs 规划
- **已建（v0）**：项目管理（文件夹选择 + 路径校验 + 自动命名）、agent 绑定（built-in + 自定义）、启动组、一键启动（确认 / 直接）、安装检测（登录 shell + 手动刷新）、原生弹窗、macOS / Windows 终端启动、面板 UI、markdown 预览（编辑/预览 tab）。
- **已建（v1）**：`read_context`/`save_context`/`context_status` + 6 情形加载/保存 + CLAUDE.md 指针 + 合并块 + 哨兵提醒 + legacy 迁移 + revision 冲突保护 + 原子写入。
- **规划 v1 余下**：配置绑定深化（model / 权限 / MCP）。
- **规划 v2**：跨 agent 可观测、agent 接力。

## 9. 关键决策 / 踩坑
- **同步弹窗被屏蔽**：Tauri v2 webview 不显示 `confirm()`/`alert()`，全部改用 dialog 插件异步 `confirm`/`message`，权限 `dialog:default`。
- **安装检测必须用登录 shell**：`which` 会漏掉 rc/profile 里设 PATH 的工具（fnm 把 claude/codex 放 .zshrc）。用 `$SHELL -ilc` 匹配终端实际环境，dev 和打包都准。
- **built-in vs 自定义**：built-in（agents.toml）不可改不可删、前端标 `built-in`；自定义（config.toml）可增删改。`update_agent`/`remove_agent` 遇 built-in id 直接拒绝。
- **`agent_command` 覆盖已移除**：项目按 agent id 绑定，命令来自 agent 预设 / 条目，无 per-project 命令覆盖。
- **agent 列表固定列宽**：built-in 标签 / 安装状态 / 操作三列固定宽，跨行对齐。
- **上下文：文件即正本，不 sync**：废弃 termana 持有 `Project.context` + 派生多份的方案。`AGENTS.md` 是正本，`CLAUDE.md` 是 `@AGENTS.md` 指针。原因：社区已有 AGENTS.md 标准 + @import，termana 重复生成引入 drift 和 `Generated by termana` 污染。
- **指针用 `@AGENTS.md` 纯文本，非 symlink**：Windows / Git 安全（symlink 在 Windows 需 Developer Mode、checkout 易变形）。
- **CLAUDE.md 指针默认建 + 告知**：可选等于没人建 -> Claude Code 读不到 -> 功能半残。
- **CLAUDE.md 已链接判定放宽**：含 `@AGENTS.md` 即视为已链接（纯指针 / import 行 / 带备注），不误清 Claude 专属备注（egg/coreos 成熟写法）。仅"整文件==`@AGENTS.md`"的窄判定会毁备注。
- **CLAUDE.md 是 symlink 不写**：`symlink_metadata` 检测；`fs::write` 对 symlink 写穿会毁目标（next.js 软链写穿把 AGENTS.md 变自指指针）。判 `symlink-skip`，留给用户手动处理。
- **情形 6 合并块 + 哨兵**：两文件不一致时，加载阶段把 CLAUDE.md 内容以可见标记追加进 AGENTS.md，附 `<!-- termana-merge-block -->` 哨兵 + 人/AI 合并指令。termana 检测哨兵做提醒，不靠 agent 自动触发（agent 不改自己的上下文文件，除非用户显式要求）。
- **提醒职责归 termana**：agent 不会因读到提示词就自动合并或提醒；哨兵检测 + 界面提醒由 termana 负责。
