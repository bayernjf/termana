# AGENTS.md — termana

供 AI coding agents（Claude Code / Codex / Cursor / Copilot 等）在本仓库工作时自动读取。

## 项目概览
termana：本地优先的终端项目启动器桌面应用（macOS / Windows）。在一个面板里管理多个项目，
为每个项目绑定一个 coding agent（Claude Code、Codex、Aider、OpenCode），内置 AGENTS.md 上下文编辑器，
一键启动终端并进入项目。落地页仓库是 `termana-landing`。

## 技术栈
| 层 | 方案 |
|---|---|
| 前端 | Vite 6 + TypeScript 5.6（无前端框架依赖，含 `marked` 渲染 Markdown） |
| 桌面 | Tauri v2（`src-tauri/`，Rust） |
| Tauri 插件 | dialog、updater、process |
| 测试 | `cargo test`（`--manifest-path src-tauri/Cargo.toml`） |

## 常用命令
```bash
npm install
npm run dev          # 前端开发服务器
npm run build        # tsc && vite build
npm run check        # tsc --noEmit
npm run tauri:dev    # 启动桌面应用
npm run tauri:build  # 打包桌面应用
npm test             # cargo test
```

## 约定
- Agent 绑定配置在 `agents.toml`，新增 / 调整内置 agent 支持时改这里。
- 涉及窗口、权限、更新的能力要同时检查 `src-tauri/` 的配置与能力声明，否则插件调用会被拒。
- 内置 AGENTS.md 编辑器是产品差异点：改动要保证用户手写上下文能被正确写入并传给 agent。
- 文档在 `docs/`（PRD、CHANGELOG、UPDATER、technical-design）；落地页仓库是 `termana-landing`。

## 不要做的事
- 不要破坏「本地优先」：项目路径与上下文不应外传。
- 不要提交 `dist/`、`src-tauri/target/` 与 `.env`。
- 不要跳过 `git pull --rebase` 直接 push。
