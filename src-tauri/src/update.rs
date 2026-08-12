use serde::{Deserialize, Serialize};

const GITHUB_API: &str = "https://api.github.com/repos/bayernjf/termana/releases/latest";
const ANNOUNCEMENTS_URL: &str =
    "https://raw.githubusercontent.com/bayernjf/termana/main/announcements.json";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub download_url: String,
    pub release_notes: String,
    pub release_url: String,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    body: String,
    assets: Vec<GitHubAsset>,
}

#[derive(Deserialize)]
struct GitHubAsset {
    browser_download_url: String,
    name: String,
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Announcement {
    pub id: String,
    pub title: String,
    pub content: String,
    pub severity: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

fn current_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Compare two version strings like "0.1.0" vs "0.2.0".
fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |s: &str| -> Vec<u32> {
        s.trim_start_matches('v')
            .split('.')
            .filter_map(|p| p.parse().ok())
            .collect()
    };
    let l = parse(latest);
    let c = parse(current);
    for i in 0..l.len().max(c.len()) {
        let lv = l.get(i).copied().unwrap_or(0);
        let cv = c.get(i).copied().unwrap_or(0);
        if lv > cv {
            return true;
        }
        if lv < cv {
            return false;
        }
    }
    false
}

/// Pick the best-matching download asset for the current platform.
/// Release assets are named like `termana_{version}_{arch}.dmg` (macOS) or
/// `termana_{version}_{arch}-setup.exe` / `_{arch}_en-US.msi` (Windows).
/// The updater bundles (`termana_{arch}.app.tar.gz`) are the last resort
/// because they are not standalone installers.
fn pick_asset_url(assets: &[GitHubAsset]) -> String {
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x64" // covers x86_64
    };

    let is_installer = |name: &str| -> bool {
        if cfg!(target_os = "macos") {
            name.ends_with(".dmg")
        } else if cfg!(target_os = "windows") {
            name.ends_with(".exe") || name.ends_with(".msi")
        } else {
            false
        }
    };

    // 1) Prefer an installer matching the current architecture.
    if let Some(asset) = assets.iter().find(|a| {
        let name = a.name.to_lowercase();
        name.contains(arch) && is_installer(&name)
    }) {
        return asset.browser_download_url.clone();
    }

    // 2) Any installer for the current OS (different arch).
    if let Some(asset) = assets.iter().find(|a| is_installer(&a.name.to_lowercase())) {
        return asset.browser_download_url.clone();
    }

    // 3) Fall back to the updater bundle for this arch.
    if let Some(asset) = assets.iter().find(|a| {
        let name = a.name.to_lowercase();
        name.contains(arch) && (name.ends_with(".zip") || name.ends_with(".tar.gz"))
    }) {
        return asset.browser_download_url.clone();
    }

    // 4) Any updater bundle.
    assets
        .iter()
        .find(|a| {
            let name = a.name.to_lowercase();
            name.ends_with(".zip") || name.ends_with(".tar.gz")
        })
        .map(|a| a.browser_download_url.clone())
        .unwrap_or_default()
}

#[tauri::command]
pub fn check_for_updates() -> Result<UpdateInfo, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("termana-updater")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("failed to create HTTP client: {e}"))?;

    let resp: GitHubRelease = client
        .get(GITHUB_API)
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .map_err(|e| format!("failed to fetch latest release: {e}"))?
        .json()
        .map_err(|e| format!("failed to parse release info: {e}"))?;

    let current = current_version();
    let latest = resp.tag_name.trim_start_matches('v').to_string();
    let has_update = is_newer(&latest, &current);

    Ok(UpdateInfo {
        current_version: current,
        latest_version: latest,
        has_update,
        download_url: pick_asset_url(&resp.assets),
        release_notes: resp.body,
        release_url: resp.html_url,
    })
}

#[tauri::command]
pub fn fetch_announcements() -> Result<Vec<Announcement>, String> {
    // In dev mode, try the local announcements.json first.
    // CARGO_MANIFEST_DIR = src-tauri/, so project root is one level up.
    #[cfg(debug_assertions)]
    {
        let local = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .map(|p| p.join("announcements.json"));

        if let Some(path) = local {
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(announcements) = serde_json::from_str::<Vec<Announcement>>(&content) {
                        return Ok(announcements);
                    }
                }
            }
        }
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent("termana-updater")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("failed to create HTTP client: {e}"))?;

    let resp = client
        .get(ANNOUNCEMENTS_URL)
        .send()
        .map_err(|e| format!("failed to fetch announcements: {e}"))?;

    if !resp.status().is_success() {
        return Ok(vec![]);
    }

    let announcements: Vec<Announcement> = resp
        .json()
        .map_err(|e| format!("failed to parse announcements: {e}"))?;

    Ok(announcements)
}
