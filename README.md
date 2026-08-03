# termana

A terminal project launcher for people who live in the terminal. Manage multiple projects in one panel, bind a coding agent (Claude Code, Codex, Aider, OpenCode, …) to each, and launch with one click - a terminal opens, `cd`s into the project, and starts the agent.

> Status: **v1** (launcher + context editor). Cross-agent observability is planned - see [Roadmap](#roadmap).

## Why

In a multi-agent world, each CLI agent vendor only manages its own config. termana sits in the seams between agents - a project control layer that belongs to no single vendor.

## Features

- **Project cards** - one click opens your system terminal, `cd`s into the project, and runs the bound agent.
- **Agent binding** - each project binds to an agent; termana resolves the launch command.
- **Install detection** - checks which agents are actually on your PATH (via a login shell, so fnm / nvm / asdf / volta setups are detected) and only lets you pick installed ones.
- **Configurable agent list** - add / edit / remove agents; the list is persisted and detected.
- **Local-first** - no account, no cloud, no server. Config is a TOML file on disk.
- **Context editor** - edit and preview each project's canonical `AGENTS.md`; termana creates a safe `CLAUDE.md` pointer and surfaces divergent files for review.

## Supported terminals

- **macOS**: Terminal.app (via `osascript`)
- **Windows**: PowerShell (via `powershell` + `CREATE_NEW_CONSOLE`, profile loaded)

## Tech stack

- **Desktop**: Tauri v2
- **Backend**: Rust
- **Frontend**: Vanilla TypeScript + Vite
- **Config**: TOML

## Getting started

Prerequisites: Rust, Node.js, and (on macOS) Xcode Command Line Tools.

```bash
npm install
npm run tauri dev
```

Build a release binary:

```bash
npm run tauri build
```

## Configuration

Stored as TOML:

- macOS: `~/Library/Application Support/termana/config.toml`
- Windows: `%APPDATA%\termana\config.toml`

```toml
[[projects]]
id = "my-app"
name = "My App"
path = "/Users/me/projects/my-app"
agent = "claude"

[[agents]]
id = "claude"
name = "Claude Code"
command = "claude"
```

The agent list is seeded with defaults (claude / codex / aider / opencode) on first run; edit it freely in the UI.

## Project structure

```
src-tauri/src/
├── lib.rs              app entry, registers commands
├── config.rs           Project/Agent/Config model + TOML persistence
├── commands.rs         projects + agents CRUD, launch
└── adapters/
    ├── agent.rs        install detection (login shell / Get-Command)
    └── terminal.rs     TerminalAdapter: macOS + Windows
src/
├── main.ts             panel UI
└── styles.css          dark theme
docs/
├── PRD.md
└── technical-design.md
```

## Roadmap

- **v0** ✅ project registry, agent binding, one-click launch, install detection, configurable agent list
- **v1** ✅ `AGENTS.md` editor, `CLAUDE.md` pointer, legacy-context migration, divergence detection and guarded reconciliation
- **v1 next** - deeper per-project config (model, permissions, MCP)
- **v2** - cross-agent observability; agent handoff with shared context

## License

Not yet licensed.
