# termana - Handoff

> Last updated: 2026-08-04 (v1 context editor implemented).

## Product

termana is a local-first Tauri desktop app for registering projects, binding a CLI coding agent to each, and launching the project in the system terminal. Groups launch several projects at once. The context editor treats each project's `AGENTS.md` as the canonical agent context and connects Claude Code through a `CLAUDE.md` `@AGENTS.md` pointer.

## Current status

- **v0 launcher:** implemented on macOS; Windows adapter exists but still needs real-machine verification.
- **v1 context editor:** implemented. The old config-owned sync model and launch-time file overwrite have been removed.
- **Working tree:** contains the v1 implementation described below; verify `git status` rather than relying on a recorded commit hash.

## Context model

- `AGENTS.md` is canonical and is edited directly.
- Missing or empty `CLAUDE.md` is atomically created as `@AGENTS.md\n` on save.
- A non-empty independent `CLAUDE.md` is promoted or appended as a visible merge block. Conversion to the pointer requires explicit confirmation; cancel keeps it unchanged.
- A symlinked `CLAUDE.md` is never written through.
- Old `Project.context` values remain deserialize-compatible as `legacy_context`, enter the editor as a migration source, and are cleared only after `AGENTS.md` saves successfully.
- `read_context` returns revisions for AGENTS / CLAUDE / legacy state. `save_context` rejects stale revisions instead of overwriting external edits.
- AGENTS, CLAUDE and config writes use same-directory temporary files and atomic replacement.

## Commands

Backend commands registered in `src-tauri/src/lib.rs`:

- Projects: `list_projects`, `add_project`, `remove_project`, `launch_project`, `path_exists`
- Context: `read_context`, `save_context`, `context_status`
- Agents: `list_agents`, `add_agent`, `update_agent`, `remove_agent`
- Groups: `list_groups`, `add_group`, `update_group`, `remove_group`, `launch_group`

Removed context commands: `get_context`, `set_context`, `sync_context`. Launching no longer writes context files.

## Frontend behavior

- Project cards show `ctx`, `divergent`, `merge`, or `migrate` state from the project files.
- The editor tracks dirty state and confirms before discarding changes.
- Markdown preview escapes raw HTML and renders links/images as inert text.
- Tauri CSP is enabled; global Tauri injection and the unused opener plugin/permission are disabled.

## Verification

```bash
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

The Rust tests cover all six AGENTS/CLAUDE states, legacy migration merging, pointer creation, conversion consent, stale revisions, and CLAUDE symlinks.

## Remaining work

- Run and package on a Windows machine.
- Harden general config error reporting and custom-agent command detection outside the v1 editor scope.
- Add deeper per-project agent settings (model / permissions / MCP), then cross-agent observability and handoff.
