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

#[cfg(test)]
mod tests {
    use super::*;

    /// Mirrors the `cfg!` choices inside `pick_asset_url` so the fixtures below
    /// are named the way real release assets are on whatever host runs the test.
    const ARCH: &str = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x64"
    };
    const INSTALLER_EXT: &str = if cfg!(target_os = "macos") { "dmg" } else { "msi" };
    const OTHER_ARCH: &str = if cfg!(target_arch = "aarch64") {
        "x64"
    } else {
        "aarch64"
    };

    fn asset(name: &str) -> GitHubAsset {
        GitHubAsset {
            browser_download_url: format!("https://example.com/{name}"),
            name: name.to_string(),
        }
    }

    #[test]
    fn compares_version_components_numerically() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("0.1.10", "0.1.9"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
    }

    #[test]
    fn tolerates_v_prefix_and_missing_components() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("0.2", "0.1.9"));
        // Absent components count as zero, so these are equal, not newer.
        assert!(!is_newer("0.1", "0.1.0"));
    }

    #[test]
    fn drops_non_numeric_version_components() {
        // Documents a trap: "0-dev" fails to parse and is silently skipped, so
        // a dev snapshot's leading zeros shift left and it never looks newer
        // than a real release.
        assert!(!is_newer("0.0.0-dev.20260812", "0.1.0"));
        // Same reason: the suffix vanishes instead of ranking below the release.
        assert!(!is_newer("0.1.0-rc1", "0.1.0"));
    }

    #[test]
    fn prefers_installer_matching_the_current_arch() {
        let assets = vec![
            asset(&format!("termana_0.1.0_{OTHER_ARCH}.{INSTALLER_EXT}")),
            asset(&format!("termana_0.1.0_{ARCH}.{INSTALLER_EXT}")),
            asset(&format!("termana_{ARCH}.app.tar.gz")),
        ];
        assert_eq!(
            pick_asset_url(&assets),
            format!("https://example.com/termana_0.1.0_{ARCH}.{INSTALLER_EXT}")
        );
    }

    #[test]
    fn falls_back_to_an_installer_for_another_arch() {
        let assets = vec![
            asset(&format!("termana_{ARCH}.app.tar.gz")),
            asset(&format!("termana_0.1.0_{OTHER_ARCH}.{INSTALLER_EXT}")),
        ];
        assert_eq!(
            pick_asset_url(&assets),
            format!("https://example.com/termana_0.1.0_{OTHER_ARCH}.{INSTALLER_EXT}")
        );
    }

    #[test]
    fn falls_back_to_the_updater_bundle_only_when_no_installer_exists() {
        let assets = vec![
            asset(&format!("termana_{OTHER_ARCH}.app.tar.gz")),
            asset(&format!("termana_{ARCH}.app.tar.gz")),
        ];
        assert_eq!(
            pick_asset_url(&assets),
            format!("https://example.com/termana_{ARCH}.app.tar.gz")
        );
    }

    #[test]
    fn returns_empty_when_nothing_is_downloadable() {
        assert_eq!(pick_asset_url(&[]), "");
        assert_eq!(pick_asset_url(&[asset("latest.json")]), "");
    }
}
