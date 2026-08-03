# termana - 产品需求文档（PRD）

## 一句话定义
termana 是一个 **agent 中立的项目控制层**：在一个面板里管理多个开发项目，每个项目绑定一个 coding agent（Claude Code / Codex / ...），点一下拉起终端直接开干；并在此基础上提供跨 agent 的上下文归一化与可观测性。

## 背景
2026 年，开发者同时使用多个 CLI coding agent（Claude Code、Codex、Aider、Gemini CLI…）。每个 agent 有自己的上下文文件格式、配置模型、权限和 MCP 接入方式。这些碎片化目前全靠手动维护，且没有任何一个 agent 厂商愿意管理竞品的配置。

## 目标用户
- 终端爱好者 / 重度 CLI 用户
- 同时维护多个项目、且在项目间使用不同 coding agent 的开发者
- 追求可复现、可版本化的 agent 工作环境的人

## 核心问题（痛点）
1. **上下文碎片化**：同一项目要维护 `CLAUDE.md` / `AGENTS.md` / `.codex` / `.cursorrules`，内容重叠且 drift。
2. **配置不迁移**：项目用哪个 agent、model、MCP、权限，全靠记忆，换机器 / 换人即丢失。
3. **跨 agent 无感知**：多个 agent 会话分散在终端窗口，看不到全局状态和 token 消耗。
4. **agent 切换无接力**：任务卡住想换 agent，context 带不过去。

## 核心价值主张
**agent 中立。** 每个 agent 厂商只管自己，termana 占据"多 agent 之间的缝隙"--这是唯一不会被单一厂商吃掉的位置。

## 非目标（明确不做）
- 不做自己的终端模拟器（drive 系统 tmux / 终端，不嵌入）。
- 不做云托管 SaaS（本地优先）。
- 不替代任何 agent 本身的能力（不自己跑模型）。
- v1 不做团队 / 多人协作。

## 功能（v0，已建）

- **项目管理**：原生文件夹选择器选目录（路径自动填充 + 项目名自动取文件夹名），路径失焦 / 回车实时校验是否存在，不存在禁用添加；删除二次确认。
- **一键启动**：`Launch ▸` 直接启动；点卡片主体二次确认后启动；`✕` 二次确认删除。启动 = 开系统终端、cd 进目录、跑绑定的 agent。
- **Agent 绑定**：built-in 预设（`agents.toml`，编译期嵌入、不可改不可删）+ 用户自定义 agent（config.toml，增删改）。项目按 agent id 绑定。
- **安装检测**：在用户实际用的 login shell 里一次性查所有 agent（fnm / nvm / volta 等 rc/profile 设置也生效），只认外部命令、不被 shell builtin 误判；手动 ↻ Refresh 重新检测。
- **启动组**：把多个项目组成一组，`Launch all` 一键全开（每个项目各开一个终端）。
- **原生弹窗**：确认 / 错误用 Tauri dialog 插件（webview 屏蔽同步 `confirm()`/`alert()`）。
- **本地优先**：无账号、无云、无服务。配置是磁盘上的 TOML 文件。

## 范围分期
| 阶段 | 内容 | 状态 |
|---|---|---|
| **v0** | 项目管理（文件夹选择 + 路径校验 + 自动命名）+ agent 绑定（built-in 预设 + 自定义）+ 一键启动（确认 / 直接）+ 启动组 + 安装检测（登录 shell + 手动刷新）+ 原生弹窗。入口层。 | 已建 |
| **v1** | 上下文归一化（写一次，派生各 agent 格式）；配置绑定深化（model / 权限 / MCP）。 | 规划 |
| **v2** | 跨 agent 可观测面板；agent 间带 context 接力。 | 规划 |

## 成功标准
- **v0**：用户能在面板里配多个项目、绑定不同 agent、一键拉起终端跑起来；能用启动组一键开多个。
- **v1**：用户为一个项目写一份 context，sync 后各 agent 格式文件正确生成；换 agent 不需要重写 context。
- **留存信号**：用户持续用 termana 启动项目（而非 alias / tmuxinator），且依赖 sync 维护 context。

## 当前完成度与上线判断
- **作为 v0 启动器（macOS）：~85%**，核心闭环跑通，可自用。
- **对照产品愿景：~30%**，核心层（v1 上下文归一化、v2 可观测）未动；launcher 层可被 alias + tmuxinator 替代，尚无不可替代价值。
- **公开发布：未就绪**--Windows 未实测、未打包签名、默认图标、边界未压测、无差异化护城河。建议先建 v1 上下文归一化再硬化上线。

## 风险
- **停在 launcher**：免费替代品（alias / tmuxinator）够用，无留存。-> 必须推进到 context 归一化层。
- **被 agent 厂商 commoditize**：单 agent 配置管理厂商会自己做。-> 押注多 agent 中立，不押注单 agent 功能。
- **adapter 维护成本**：CLI agent 演进快。-> 薄 adapter 层 + 社区贡献。
- **Windows 未验证**：跨平台代码在 Mac 上无法编译 / 测试 Windows 分支。-> 上线前必须在 Windows 机器实测。
