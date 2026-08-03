# termana - 技术方案

## 1. 架构总览
本地优先的桌面应用：
- **前端**：vanilla TS + Vite，渲染在 Tauri webview 里（项目 / 组 / agent 面板）。
- **后端**：Rust（Tauri 后端），负责配置持久化、终端启动、路径校验、安装检测、（未来）context sync。
- **终端**：不嵌入，drive 用户系统自带的终端（macOS Terminal.app / Windows PowerShell）。

```
┌─────────────────────────────────┐
│  Tauri Webview (vanilla TS UI)  │  Projects / Groups / Agents
└──────────────┬──────────────────┘
               │ invoke (Tauri commands) + dialog plugin
┌──────────────▼──────────────────┐
│  Rust 后端                       │
│  ├─ config.rs    Project/Agent/Group + TOML 持久化 + builtin_agents()
│  ├─ commands.rs  project/agent/group CRUD · launch · path_exists · resolve_and_launch
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
| 桌面框架 | Tauri v2（非 Electron） | 后端要干系统活（spawn 进程），Rust 合身；体积小、启动快；权限白名单更安全。不嵌终端使 Electron 的 xterm.js 优势失效。 |
| 终端策略 | drive 系统终端（非嵌入） | 不造终端模拟器；尊重终端爱好者已有的 tmux / 终端环境；工程量从一年降到几周。 |
| 前端 | vanilla TS | 面板 UI 简单，无需框架；bundle 极小。UI 复杂化后可换 Svelte。 |
| 弹窗 | tauri-plugin-dialog | webview 屏蔽同步 `confirm()`/`alert()`，确认 / 错误改用异步 `confirm`/`message`；文件夹选择用 `open`。`dialog:default` 权限。 |
| 配置格式 | TOML | 终端工具惯例，人可读可手改。 |
| 部署 | 本地优先，无后端服务 | 核心动作是 spawn 本地进程，浏览器沙箱做不到；离线、无账号是卖点。 |

> dev server 端口为 **1442**（非 Tauri 默认 1420，因开发机 1420 被占），见 [vite.config.ts](../vite.config.ts) 与 [tauri.conf.json](../src-tauri/tauri.conf.json) `devUrl`。

## 3. 模块结构（已建）
```
src-tauri/src/
├── lib.rs              应用入口，注册 15 个命令 + dialog/opener 插件
├── config.rs           Project / Agent / Group 模型 + TOML 读写 + builtin_agents()（agents.toml）+ slugify
├── commands.rs         projects: list/add/remove/launch · agents: list/add/update/remove ·
│                       groups: list/add/update/remove/launch · path_exists · resolve_and_launch（共享）
└── adapters/
    ├── mod.rs
    ├── agent.rs        installed_status() 安装检测（login shell / Get-Command）
    └── terminal.rs     TerminalAdapter trait + MacTerminal / WindowsPowerShell
agents.toml             built-in 预设（纯 key-value：`"名称" = "命令"`，include_str! 编译期嵌入）
src/
├── main.ts             面板 UI（项目 / 组 / agent）+ 事件 + 路径校验 + 异步弹窗
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

built-in 预设在项目根 [agents.toml](../agents.toml)：
```toml
"Claude Code" = "claude"
"Codex" = "codex"
"Aider" = "aider"
"OpenCode" = "opencode"
```

v1 规划扩展（project）：
```toml
model = "claude-sonnet-5"
mcp_servers = ["filesystem", "github"]
permissions = ["read", "write"]
context_file = "termana.context.md"   # 归一化上下文源
```

## 5. 关键流程

### 启动项目（launch_project）
1. 前端 `invoke("launch_project", { id })`
2. `resolve_and_launch(cfg, id)`：找 project，解析命令（built-in agent > 自定义 agent > 原始 id）
3. `default_terminal()` 按 OS 选适配器，spawn 终端 cd + 跑命令
   - macOS：`osascript` -> `tell application "Terminal" to do script "cd '<path>' && <cmd>"`
   - Windows：`powershell -NoExit -Command "Set-Location '<path>'; <cmd>"`（`CREATE_NEW_CONSOLE` 开新窗口，加载 profile）

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

## 6. 适配器设计
终端与 agent 都走 adapter 模式，加新终端 / 新 agent = 加一个文件：
- **TerminalAdapter** trait：`launch(dir, command)`。已实现：`MacTerminal`、`WindowsPowerShell`。未来：iTerm2、Ghostty、Alacritty、Kitty。
- **Agent 列表**：built-in 预设从 `agents.toml` 加载（标记 built-in、不可改不可删）；自定义 agent 存 config、可增删改。`list_agents` 合并两者并做安装检测。未来扩展为 AgentAdapter 负责 context emit。
- **resolve_and_launch**：project -> agent command -> terminal，project 与 group 启动共用。

## 7. 跨平台注意
- macOS 分支 `#[cfg(target_os = "macos")]`，Windows 分支 `#[cfg(target_os = "windows")]`，按 OS 编译。
- **Windows 适配器在 macOS 上无法编译 / 测试**（cfg 跳过），需在 Windows 机器验证。
- 路径带空格：macOS 用 AppleScript `quoted form of`；Windows 由 `powershell -Command` + 单引号处理。

## 8. 已建 vs 规划
- **已建（v0）**：项目管理（文件夹选择 + 路径校验 + 自动命名）、agent 绑定（built-in + 自定义）、启动组、一键启动（确认 / 直接）、安装检测（登录 shell + 手动刷新）、原生弹窗、macOS / Windows 终端启动、面板 UI。
- **已建（v1 增量）**：上下文归一化--per-project agent-neutral context（`Project.context`）+ 手动 `sync_context` + 启动自动 sync（`resolve_and_launch` 内 `write_context_file`，best-effort）+ `ctx` 徽章。`context_file_for` 映射 agent->文件（claude->CLAUDE.md 等）。
- **规划 v1 余下**：配置绑定深化（model / 权限 / MCP）、多格式 emit（一份 context 写多个 agent 文件）。
- **规划 v2**：跨 agent 可观测、agent 接力。

## 9. 关键决策 / 踩坑
- **同步弹窗被屏蔽**：Tauri v2 webview 不显示 `confirm()`/`alert()`，全部改用 dialog 插件异步 `confirm`/`message`，权限 `dialog:default`。
- **安装检测必须用登录 shell**：`which` 会漏掉 rc/profile 里设 PATH 的工具（fnm 把 claude/codex 放 .zshrc）。用 `$SHELL -ilc` 匹配终端实际环境，dev 和打包都准。
- **built-in vs 自定义**：built-in（agents.toml）不可改不可删、前端标 `built-in`；自定义（config.toml）可增删改。`update_agent`/`remove_agent` 遇 built-in id 直接拒绝。
- **`agent_command` 覆盖已移除**：项目按 agent id 绑定，命令来自 agent 预设 / 条目，无 per-project 命令覆盖。
- **agent 列表固定列宽**：built-in 标签 / 安装状态 / 操作三列固定宽，跨行对齐，不受名字长度或安装状态影响。
