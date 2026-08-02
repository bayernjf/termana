# termana · Handoff

> Last updated: 2026-08-03. Reflects the state after the `feat: add launch groups` commit.

## What it is

termana is a **terminal project launcher for multi-agent workflows**. A Tauri desktop app where you register projects, bind a coding agent (Claude Code / Codex / Aider / OpenCode / custom) to each, and launch with one click — a terminal opens, `cd`s into the project, and runs the agent. You can also group projects and launch them all at once.

Local-first: no account, no cloud, no server. Config is a TOML file on disk.

## Status

**v0 (launcher) — done.** The agent-neutral context-normalization layer and cross-agent observability are the v1/v2 roadmap (see [docs/PRD.md](docs/PRD.md)), not yet built.

### What works (v0)
- **Projects**: add (with native folder picker, path validation, name auto-filled from folder), remove, launch.
- **Project launch**: `Launch ▸` hint = direct launch; clicking the card body = confirm-then-launch; `✕` = confirm-remove.
- **Agents**: built-in presets loaded from [agents.toml](agents.toml) (immutable, compile-time embedded) + user-defined custom agents (config, mutable). Install detection via a login shell (fnm/nvm/asdf/volta aware), with a manual ↻ Refresh button.
- **Launch groups**: group multiple projects, `Launch all ▸` opens a terminal per project.
- **Dialogs**: native Tauri confirm/message dialogs (sync `confirm()`/`alert()` are blocked by the webview — see Gotchas).

## Tech stack

| Layer | Choice |
|---|---|
| Desktop | Tauri v2 |
| Backend | Rust (edition 2021) |
| Frontend | Vanilla TypeScript + Vite 6 (no framework) |
| Config | TOML |
| Terminals | macOS Terminal.app (osascript) · Windows PowerShell (`powershell` + `CREATE_NEW_CONSOLE`) |

## Run

```bash
npm install
npm run tauri dev      # dev app on http://localhost:1442
npm run tauri build    # release binary
```

Prerequisites: Rust, Node.js, Xcode CLT (macOS).

**Note:** the dev server uses port **1442** (not the Tauri default 1420), because 1420 was occupied by another app on the dev machine. Configured in [vite.config.ts](vite.config.ts) and [src-tauri/tauri.conf.json](src-tauri/tauri.conf.json) `devUrl`.

## Config

- macOS: `~/Library/Application Support/termana/config.toml`
- Windows: `%APPDATA%\termana\config.toml`

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

[[agents]]        # user-defined (custom) agents only
id = "gemini"
name = "Gemini"
command = "gemini"
```

Built-in presets are **not** in this file — they come from [agents.toml](agents.toml) (root), embedded at compile time via `include_str!`.

## Architecture

```
src-tauri/src/
├── lib.rs              app entry; registers all Tauri commands + plugins
├── config.rs           Project / Agent / Group models, TOML load/save, builtin_agents() (from agents.toml), slugify
├── commands.rs         project + agent + group CRUD, launch_project, launch_group, path_exists, resolve_and_launch (shared)
└── adapters/
    ├── agent.rs        installed_status() — install detection (login shell / Get-Command, external commands only)
    └── terminal.rs     TerminalAdapter trait: MacTerminal (osascript) / WindowsPowerShell
agents.toml             built-in agent presets (key-value: "Name" = "command")
src/
├── main.ts             panel UI (projects, groups, agents) + all event handlers
└── styles.css          dark theme
docs/
├── PRD.md
└── technical-design.md
```

### Key flows
- **Launch project**: `launch_project(id)` → `resolve_and_launch` finds the project, resolves the agent command (built-in > custom > raw id), calls the platform terminal adapter → opens terminal, `cd` + run.
- **Launch group**: `launch_group(id)` → loops the group's project_ids, `resolve_and_launch` each → one terminal per project.
- **Install detection**: `list_agents` → `installed_status(commands)` runs one login shell (`$SHELL -ilc` on unix, `powershell -Command` on Windows) checking `command -v` / `Get-Command -CommandType Application` for every agent. Only external commands (absolute path / Application) count — shell builtins like `continue` are not mistaken for installed agents.

## Gotchas / key decisions

- **Sync dialogs are blocked**: Tauri v2's webview does not show synchronous `confirm()`/`alert()`. All confirms/alerts use `@tauri-apps/plugin-dialog` (`confirm`/`message`), with the `dialog:default` capability in [src-tauri/capabilities/default.json](src-tauri/capabilities/default.json).
- **Install detection must use a login shell**: a plain `which` misses tools whose PATH is set in rc/profile (fnm puts `claude`/`codex` in `.zshrc`). Detection runs `$SHELL -ilc` so it matches what the launched terminal sees. Works in both `tauri dev` and a packaged app.
- **Built-in vs custom agents**: built-ins (agents.toml) are immutable in the UI (no edit/delete, marked `built-in`); custom agents (config.toml) are fully editable. `list_agents` merges both; `update_agent`/`remove_agent` reject built-in ids.
- **Agent list columns are fixed-width** so built-in tag / install status / actions line up across rows regardless of name length or install state.
- **Windows terminal code is unverified on this machine** (developed on macOS; the `#[cfg(target_os="windows")]` branch is skipped here). Needs a real Windows test.
- **`agent_command` override was removed** — projects bind an agent (by id), and the command comes from the agent's preset/entry. No per-project command override.

## Git state

- Repo: https://github.com/bayernjf/termana (public)
- `origin/main` is at the **initial commit**; **8 newer commits are local and not yet pushed** (built-in presets, folder picker, docs, refresh button, column alignment, Pi command, card confirm/async dialogs, launch groups).
- Commits follow conventional commits, English messages, authored by `bayernjf` (no AI co-author attribution).

```
a113f2b feat: add launch groups
32a7fe4 feat(project): confirm card click and direct launch hint
76728bc chore(agents): set Pi preset command to pi
443f447 feat(ui): align agent list columns
d579d7f feat(agents): add manual re-detect refresh button
09af068 docs: update PRD, technical design, and README
4133a27 feat(project): add folder picker, path validation, and auto-filled name
8e3069b feat(agents): load built-in presets from file and split built-in/custom agents
7b95e13 Initial commit: termana v0   ← origin/main (pushed)
```

## Roadmap (not started)

- **v1**: context normalization — write one agent-neutral project context, emit `CLAUDE.md` / `AGENTS.md` / `.codex` / `.cursorrules`. Deeper per-project config (model, permissions, MCP).
- **v2**: cross-agent observability; agent handoff with shared context.

See [docs/PRD.md](docs/PRD.md) for the full product rationale (the core thesis: termana exists because the multi-agent world has seams no single vendor fills).
