use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    pub agent: String,
    pub agent_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub projects: Vec<Project>,
    #[serde(default)]
    pub agents: Vec<Agent>,
}

/// Built-in default agents, used to seed the list on first run. The user
/// can then add / edit / delete freely; the list is persisted in config.
pub fn default_agents() -> Vec<Agent> {
    vec![
        Agent { id: "claude".into(), name: "Claude Code".into(), command: "claude".into() },
        Agent { id: "codex".into(), name: "Codex".into(), command: "codex".into() },
        Agent { id: "aider".into(), name: "Aider".into(), command: "aider".into() },
        Agent { id: "opencode".into(), name: "OpenCode".into(), command: "opencode".into() },
    ]
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

/// Agents from config, or the built-in defaults if none configured yet.
pub fn load_agents_or_seed() -> Vec<Agent> {
    let cfg = load();
    if cfg.agents.is_empty() {
        default_agents()
    } else {
        cfg.agents
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
