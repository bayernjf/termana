use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub path: String,
    pub agent: String,
    /// Read-only migration source from the pre-v1 sync model. New context is
    /// stored in AGENTS.md; this value is cleared after a successful save.
    #[serde(default, rename = "context", skip_serializing_if = "Option::is_none")]
    pub legacy_context: Option<String>,
    /// Epoch millis when the project was added. 0 for pre-v1 configs.
    #[serde(default)]
    pub created_at: i64,
    /// Epoch millis of the last launch, if any.
    #[serde(default)]
    pub last_launched: Option<i64>,
    /// Number of times the project has been launched.
    #[serde(default)]
    pub launch_count: u32,
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
    let map: std::collections::HashMap<String, String> = toml::from_str(raw).unwrap_or_default();
    let mut agents: Vec<Agent> = map
        .into_iter()
        .map(|(name, command)| Agent {
            id: slugify(&name),
            name,
            command,
        })
        .collect();
    // Deterministic order (HashMap iteration is random).
    agents.sort_by_key(|agent| agent.name.to_lowercase());
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
    let parent = path
        .parent()
        .ok_or_else(|| "config path has no parent directory".to_string())?;
    let mut temp = tempfile::Builder::new()
        .prefix(".termana-config-")
        .tempfile_in(parent)
        .map_err(|e| e.to_string())?;
    temp.write_all(s.as_bytes()).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(&path).map_err(|e| e.error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRE_V1: &str = r#"
        [[projects]]
        id = "app"
        name = "App"
        path = "/tmp/app"
        agent = "claude-code"
    "#;

    #[test]
    fn parses_pre_v1_config_with_defaults() {
        let cfg: Config = toml::from_str(PRE_V1).unwrap();
        let project = &cfg.projects[0];

        assert_eq!(project.id, "app");
        assert_eq!(project.agent, "claude-code");
        // Fields added after v0 must default rather than fail the whole parse.
        assert_eq!(project.created_at, 0);
        assert_eq!(project.last_launched, None);
        assert_eq!(project.launch_count, 0);
        assert_eq!(project.legacy_context, None);
        assert!(cfg.agents.is_empty());
        assert!(cfg.groups.is_empty());
    }

    #[test]
    fn maps_legacy_context_field() {
        let raw = format!("{PRE_V1}\n        context = \"Legacy notes\"\n");
        let cfg: Config = toml::from_str(&raw).unwrap();

        // The TOML key stays `context`; the field is renamed to mark it read-only.
        assert_eq!(
            cfg.projects[0].legacy_context.as_deref(),
            Some("Legacy notes")
        );

        // Once migrated the key must disappear, not persist as an empty string.
        let mut migrated = cfg.clone();
        migrated.projects[0].legacy_context = None;
        let out = toml::to_string(&migrated).unwrap();
        assert!(!out.contains("context"), "legacy key survived: {out}");
    }

    #[test]
    fn ignores_unknown_keys() {
        // Forward compatibility: a config written by a newer termana must still load.
        let cfg: Config =
            toml::from_str("unknown_top_level = 1\n[[projects]]\nid = \"a\"\nname = \"A\"\npath = \"/tmp\"\nagent = \"codex\"\nfuture_field = true\n").unwrap();
        assert_eq!(cfg.projects.len(), 1);
    }

    #[test]
    fn malformed_toml_is_rejected_so_load_falls_back_to_default() {
        // load() turns a parse error into Config::default(); this covers the parse half,
        // since load() itself reads the real user-global config path.
        assert!(toml::from_str::<Config>("[[projects]]\nid = ").is_err());
        assert!(Config::default().projects.is_empty());
    }

    #[test]
    fn builtin_agents_are_parsed_and_sorted() {
        let agents = builtin_agents();

        let claude = agents
            .iter()
            .find(|a| a.name == "Claude Code")
            .expect("Claude Code preset missing");
        assert_eq!(claude.id, "claude-code");
        assert_eq!(claude.command, "claude");

        // HashMap iteration is random, so builtin_agents() must impose an order.
        let names: Vec<String> = agents.iter().map(|a| a.name.to_lowercase()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);

        assert!(is_builtin_id("claude-code"));
        assert!(!is_builtin_id("my-custom-agent"));
    }

    #[test]
    fn slugify_lowercases_and_replaces_non_alphanumeric() {
        assert_eq!(slugify("Claude Code"), "claude-code");
        assert_eq!(slugify("OpenCode"), "opencode");
        // Runs of separators are NOT collapsed — one dash per non-alphanumeric char.
        assert_eq!(slugify("C++ Bot"), "c---bot");
        // Leading/trailing separators are trimmed, and an id is never empty.
        assert_eq!(slugify("  "), "item");
        assert_eq!(slugify(""), "item");
    }
}
