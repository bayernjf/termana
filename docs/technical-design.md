# termana — 技术方案

## 1. 架构总览
本地优先的桌面应用：
- **前端**：vanilla TS + Vite，渲染在 Tauri webview 里（项目列表面板）。
- **后端**：Rust（Tauri 后端），负责配置持久化、终端启动、（未来）context sync。
- **终端**：不嵌入，drive 用户系统自带的终端（macOS Terminal.app / Windows Terminal）。

```
┌─────────────────────────────────┐
│  Tauri Webview (vanilla TS UI)  │  项目列表 / 配置表单
└──────────────┬──────────────────┘
               │ invoke (Tauri commands)
┌──────────────▼──────────────────┐
│  Rust 后端                       │
│  ├─ config.rs    模型 + TOML 持久化│
│  ├─ commands.rs  Tauri 命令入口   │
│  └─ adapters/                   │
│     ├─ agent.rs    agent 预设    │
│     └─ terminal.rs 终端适配器     │
└──────────────┬──────────────────┘
               │ spawn
        ┌──────▼──────┐
        │ 系统终端      │ macOS Terminal.app / Windows wt.exe
        └─────────────┘
```

## 2. 技术栈选型与理由
| 决策 | 选择 | 理由 |
|---|---|---|
| 桌面框架 | Tauri v2（非 Electron） | 后端要干系统活（spawn 进程），Rust 合身；一种语言覆盖 GUI / 未来 TUI / web 后端；体积小、启动快；权限白名单更安全。不嵌终端使 Electron 的 xterm.js 优势失效。 |
| 终端策略 | drive 系统终端（非嵌入） | 不造终端模拟器；尊重终端爱好者已有的 tmux / 终端环境；工程量从一年降到几周。 |
| 前端 | vanilla TS | 面板 UI 简单，无需框架；bundle 极小。UI 复杂化后可换 Svelte。 |
| 配置格式 | TOML | 终端工具惯例，人可读可手改。 |
| 部署 | 本地优先，无后端服务 | 核心动作是 spawn 本地进程，浏览器沙箱做不到；离线、无账号是卖点。 |

## 3. 模块结构（已建）
```
src-tauri/src/
├── lib.rs              应用入口，注册 5 个命令
├── config.rs           Project/Config 模型 + TOML 读写
├── commands.rs         projects: list/add/remove/launch · agents: list/add/update/remove
└── adapters/
    ├── mod.rs
    ├── agent.rs        AgentPreset（claude/codex/aider），command_for()
    └── terminal.rs     TerminalAdapter trait + MacTerminal / WindowsPowerShell
src/
├── main.ts             面板 UI + 事件绑定
└── styles.css          暗色主题
```

## 4. 数据模型
config 路径：`~/Library/Application Support/termana/config.toml`（macOS）/ `%APPDATA%\termana\config.toml`（Windows）

当前（v0）：
```toml
[[projects]]
id = "my-app"
name = "My App"
path = "/Users/me/projects/my-app"
agent = "claude"
agent_command = ""   # 可选，覆盖预设命令
```

v1 规划扩展：
```toml
[[projects]]
# ... 上面字段
model = "claude-sonnet-5"
mcp_servers = ["filesystem", "github"]
permissions = ["read", "write"]
context_file = "termana.context.md"   # 归一化上下文源
```

## 5. 关键流程

### 启动项目（launch_project）
1. 前端 `invoke("launch_project", { id })`
2. 后端读 config，找 project
3. 解析命令：`agent_command` 优先，否则 agent preset
4. `default_terminal()` 按 OS 选适配器
5. 适配器 spawn 系统终端，cd + 跑命令
   - macOS：`osascript` → `tell application "Terminal" to do script "cd '<path>' && <cmd>"`
   - Windows：`powershell -NoExit -Command "Set-Location '<path>'; <cmd>"`（`CREATE_NEW_CONSOLE` 开新窗口，加载 profile）

### 上下文归一化（v1 规划，未建）
1. 用户在 termana 维护一份 agent-neutral context（markdown）
2. sync 时按项目绑定的 agent，派生 `CLAUDE.md` / `AGENTS.md` / `.codex/` / `.cursorrules`
3. 每个 agent 一个 emitter（与 terminal adapter 同构），知道目标格式

## 6. 适配器设计
终端与 agent 都走 adapter 模式，加新终端 / 新 agent = 加一个文件：
- **TerminalAdapter** trait：`launch(dir, command)`。已实现：`MacTerminal`、`WindowsPowerShell`。未来：iTerm2、Ghostty、Alacritty、Kitty。
- **Agent 列表**：id / name / command，存 config、首次用默认（claude/codex/aider/opencode）填充，用户可增删改；`list_agents` 对列表里每个命令做安装检测。未来扩展为 AgentAdapter 负责 context emit。
- **安装检测**：`list_agents` 一次性在用户实际用的 shell 里检查所有 agent（unix：`$SHELL -ilc` 跑 `command -v` 且只认返回绝对路径的外部命令；windows：`powershell -Command` 跑 `Get-Command -CommandType Application`，加载 profile），匹配终端拉起时实际看到的 PATH（fnm / nvm / volta 等 rc/profile 设置也生效），不会被 shell builtin（如 `continue`）误判。前端只让选已装的 agent。预设含 claude / codex / aider / opencode。

## 7. 跨平台注意
- macOS 分支 `#[cfg(target_os = "macos")]`，Windows 分支 `#[cfg(target_os = "windows")]`，按 OS 编译。
- **Windows 适配器在 macOS 上无法编译 / 测试**（cfg 跳过），需在 Windows 机器验证。
- 路径带空格：macOS 用 AppleScript `quoted form of` 处理；Windows 由 `wt.exe -d` 接收。

## 8. 已建 vs 规划
- **已建（v0）**：项目 CRUD、agent 绑定（binary 级）、macOS / Windows 终端启动、面板 UI。Rust + 前端均编译通过。
- **规划 v1**：上下文归一化、配置绑定深化（model / 权限 / MCP）。
- **规划 v2**：跨 agent 可观测、agent 接力。
