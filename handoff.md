# termana - Handoff

> Last updated: 2026-09-09 (test suite + CI added; version/asset-selection tests).

## Product

termana is a local-first Tauri desktop app for registering projects, binding a CLI coding agent to each, and launching the project in the system terminal. Groups launch several projects at once. The context editor treats each project's `AGENTS.md` as the canonical agent context and connects Claude Code through a `CLAUDE.md` `@AGENTS.md` pointer.

## Current status

- **v0 launcher:** implemented on macOS; Windows adapter exists but still needs real-machine verification.
- **v1 context editor:** implemented. The old config-owned sync model and launch-time file overwrite have been removed.
- **v1.1 updates & announcements:** implemented. Update check fetches the latest GitHub release version. Announcements are fetched from the repo's `announcements.json` (with a local-dev fallback), displayed in a bell icon dropdown, and dismissible per-id via localStorage.
- **Tests & CI:** 24 Rust unit tests pass locally and in GitHub Actions (`.github/workflows/ci.yml`, on push/PR to `main`/`dev`). `npm test` runs them, `npm run check` type-checks the frontend. CI runs on `macos-latest` because `adapters::terminal::default_terminal()` has a `compile_error!` on non-macOS/Windows targets; the crate does not compile on Linux.
- **Measured line coverage (Rust, `cargo llvm-cov`):** 52% overall — `config.rs` 71%, `update.rs` 61%, `commands.rs` 52%; the adapters, `lib.rs` and `main.rs` are untested IO/wiring. `src/main.ts` has no test runner; `tsc --noEmit` is its only automated check. Coverage is deliberately not gated in CI.
- **Working tree:** clean; `dev` is 4 commits ahead of `origin/dev` (`b73ff74` agent-resolution refactor, `5f3eda4` config/agent tests, `fa02d00` CI, `42e8417` version/asset tests), unpushed. Verify `git status` rather than relying on these hashes.

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
- Updates & announcements: `check_for_updates`, `fetch_announcements`

Removed context commands: `get_context`, `set_context`, `sync_context`. Launching no longer writes context files.

## Frontend behavior

- Project cards show `ctx`, `divergent`, `merge`, or `migrate` state from the project files.
- The editor tracks dirty state and confirms before discarding changes.
- Markdown preview escapes raw HTML and renders links/images as inert text.
- Tauri CSP is enabled; global Tauri injection and the unused opener plugin/permission are disabled.
- An announcement bell icon in the title bar shows an unread badge. Clicking opens a dropdown with dismissible Markdown announcements. Closed announcements are persisted in localStorage.

## Verification

```bash
npm run build
npm test          # cargo test --manifest-path src-tauri/Cargo.toml
npm run check     # tsc --noEmit (frontend's only automated signal)
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

The Rust tests cover all six AGENTS/CLAUDE states, legacy migration merging, pointer creation, conversion consent, stale revisions, and CLAUDE symlinks; config parsing (defaults, legacy `context`, unknown keys, slugify); agent command resolution (built-in vs custom vs raw id, id-collision suffixing); and the updater's `is_newer` / `pick_asset_url` pure logic. Agent resolution was extracted from `resolve_and_launch` into a pure `resolve_agent` helper so it can be tested without opening a terminal.

Known `is_newer` quirk pinned by tests: non-numeric version segments are silently dropped, so `0.1.0-rc1` never compares newer than `0.1.0` and dev snapshots never prompt. Change this if prereleases should ever be offered.

## Remaining work

- Push `dev` (or open a PR) so the new CI workflow gets its first real run.
- Run and package on a Windows machine.
- Harden general config error reporting and custom-agent command detection outside the v1 editor scope.
- Add deeper per-project agent settings (model / permissions / MCP), then cross-agent observability and handoff.
