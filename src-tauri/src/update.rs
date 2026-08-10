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
fn pick_asset_url(assets: &[GitHubAsset]) -> String {
    let platform_asset = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    };

    // Prefer a platform-specific .zip/.dmg/.msi/.exe, else fall back to the release page.
    assets
        .iter()
        .find(|a| {
            let name = a.name.to_lowercase();
            name.contains(platform_asset)
                && (name.ends_with(".zip")
                    || name.ends_with(".dmg")
                    || name.ends_with(".msi")
                    || name.ends_with(".exe")
                    || name.ends_with(".app.tar.gz"))
        })
        .or_else(|| {
            assets
                .iter()
                .find(|a| {
                    let name = a.name.to_lowercase();
                    name.ends_with(".zip") || name.ends_with(".tar.gz")
                })
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
