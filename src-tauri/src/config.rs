use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    pub agent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
    pub project_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub projects: Vec<Project>,
    /// User-defined (custom) agents only. Built-in presets come from
    /// `agents.toml` via `builtin_agents()`, not from here.
    #[serde(default)]
    pub agents: Vec<Agent>,
    #[serde(default)]
    pub groups: Vec<Group>,
}

/// Built-in agent presets, loaded from `agents.toml` (embedded at compile
/// time via `include_str!`). These ship with termana and cannot be modified
/// or deleted by the user. The file is a flat key-value map:
/// `"Display Name" = "command"`.
pub fn builtin_agents() -> Vec<Agent> {
    let raw = include_str!("../../agents.toml");
    let map: std::collections::HashMap<String, String> =
        toml::from_str(raw).unwrap_or_default();
    let mut agents: Vec<Agent> = map
        .into_iter()
        .map(|(name, command)| Agent {
            id: slugify(&name),
            name,
            command,
        })
        .collect();
    // Deterministic order (HashMap iteration is random).
    agents.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    agents
}

/// Whether an agent id refers to a built-in preset.
pub fn is_builtin_id(id: &str) -> bool {
    builtin_agents().iter().any(|a| a.id == id)
}

pub fn slugify(s: &str) -> String {
    let base: String = s
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = base.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "item".to_string()
    } else {
        trimmed
    }
}

/// Cross-platform config location:
///   macOS:   ~/Library/Application Support/termana/config.toml
///   Windows: %APPDATA%\termana\config.toml
fn config_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("termana").join("config.toml")
}

pub fn load() -> Config {
    match std::fs::read_to_string(config_path()) {
        Ok(s) => toml::from_str(&s).unwrap_or_default(),
        Err(_) => Config::default(),
    }
}

pub fn save(cfg: &Config) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let s = toml::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    std::fs::write(path, s).map_err(|e| e.to_string())
}
