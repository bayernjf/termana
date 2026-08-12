# termana — Updater & Release Pipeline

> Last updated: 2026-08-12 (introducing `tauri-plugin-updater` + GitHub Actions release flow).

## Why

termana ships via **GitHub Releases** with no Apple Developer ID and no EV code-signing certificate. To keep installation friction low (users get an in-app "update available" toast that downloads, verifies, and installs) we adopt Tauri's official `tauri-plugin-updater`, which signs the **update bundle** (not the OS binary) with a self-managed ed25519 keypair.

This gives us:

- **In-app auto-update** with cryptographic verification (the user's app refuses updates that don't match our public key).
- **No code-signing certificate** required for the updater itself.
- **First install** still has to be done manually from the GitHub Releases page. macOS / Windows will show a one-time "unidentified developer" warning, which we document in the README.

## Two "signings" — keep them straight

| Type | What it signs | What it's for | Needs a cert? |
|------|---------------|---------------|---------------|
| **Updater bundle signature** (`tauri signer`) | Each release's `.tar.gz` / `.zip` updater artifact | Tauri updater verifies update origin | **No** — self-generated ed25519 keypair |
| **OS code signature** (`codesign` / `signtool`) | The `.app` / `.exe` binary itself | macOS Gatekeeper / Windows SmartScreen first-run trust | **Yes** — Apple Developer ID ($99/yr), Azure Trusted Signing, or EV cert |

We adopt the first; the second is documented in the README and left for whoever funds it.

## Pipeline

```
git tag v0.2.0
   │
   ▼
GitHub Actions (release.yml) — matrix: macos-latest, windows-latest
   │
   ├── tauri-apps/tauri-action
   │     ├── npm ci
   │     ├── npm run tauri build
   │     ├── signs updater artifacts with TAURI_SIGNING_PRIVATE_KEY
   │     ├── generates latest.json
   │     └── attaches .dmg / .msi / .app.tar.gz / .zip + latest.json to release
   │
   ▼
GitHub Release v0.2.0 (draft → published manually)
   │
   ▼
User clicks "↻ 检查更新" in-app
   │
   ▼
check_for_updates → returns "v0.2.0 available"
   │
   ▼
User clicks "立即下载并安装"
   │
   ▼
tauri-plugin-updater:
   1. fetches latest.json from GitHub release
   2. verifies ed25519 signature against pubkey in tauri.conf.json
   3. downloads the platform-matching artifact
   4. installs (replaces binary on disk)
   5. relaunches the app
```

## One-time setup (developer machine)

The release pipeline needs an ed25519 keypair. **The private key never leaves GitHub Secrets.**

```bash
# generate once, on a secure machine
npx @tauri-apps/cli signer generate -w ~/.tauri/termana.key

# output:
#   Public Key: <base64 — paste into tauri.conf.json plugins.updater.pubkey>
#   Private Key: <PEM — paste whole block into GitHub Secret TAURI_SIGNING_PRIVATE_KEY>
#   Password: <you set this — paste into GitHub Secret TAURI_SIGNING_PRIVATE_KEY_PASSWORD>

# ⚠️  backup the .key file offline. Losing the private key = no more updates ever.
```

### GitHub Secrets

In the termana repo → Settings → Secrets and variables → Actions, add:

| Secret | Value |
|--------|-------|
| `TAURI_SIGNING_PRIVATE_KEY` | full PEM, including `-----BEGIN ...-----` / `-----END ...-----` lines |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | password you set during `signer generate` |

### `tauri.conf.json` placeholder

`src-tauri/tauri.conf.json` has a `<REPLACE_ME: tauri signer generate output>` placeholder for `plugins.updater.pubkey`. Replace it with the **public** key (single base64 line) the generator prints. This is safe to commit — the matching private key is what would compromise updates, and it never enters the repo.

## Build & verify locally

```bash
# 1. install JS deps
npm install

# 2. type-check + vite build (frontend)
npm run check        # tsc --noEmit
npm run build        # tsc + vite build

# 3. rust type-check
cargo check --manifest-path src-tauri/Cargo.toml

# 4. dry-run a release (no signing locally)
npx @tauri-apps/cli signer sign \
  --private-key-path ~/.tauri/termana.key \
  src-tauri/target/release/bundle/macos/termana.app.tar.gz
```

## Why the existing `update.rs` stays

The existing `check_for_updates` Tauri command and the bell-icon flow keep working — they only fetch `tag_name` + asset list and surface "v0.2.0 available" to the UI. The plugin takes over from there: it does the *download*, *signature verify*, and *install*. We do not delete the manual-check command, because:

- The bell icon needs version info without forcing a download.
- Power users who prefer to update from the browser can still click through to the release page.
- It's the same version data the plugin would fetch, so there's no consistency cost.

## CI workflow

`.github/workflows/release.yml` triggers on `v*` tags, runs `tauri-action` on macOS + Windows runners, signs updater artifacts with the secrets, and uploads a draft release. The developer reviews the draft and publishes.

## Remaining work

- Replace the bell-icon dropdown's "v0.2.0" hint with a one-click "立即下载并安装" button that calls `tauri-plugin-updater`'s `downloadAndInstall()` and `relaunch()`.
- (Future, when funded) Add an Apple Developer ID + notarization step to the macOS job to silence Gatekeeper on first install. Until then, README documents the right-click → Open workaround.
