use crate::adapters::{agent, terminal};
use crate::config::{self, Agent, Group, Project};
use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub command: String,
    pub installed: bool,
    pub built_in: bool,
}

fn unique_project_id(projects: &[Project], base: &str) -> String {
    let mut id = base.to_string();
    let mut n = 2;
    while projects.iter().any(|p| p.id == id) {
        id = format!("{}-{}", base, n);
        n += 1;
    }
    id
}

fn unique_agent_id(agents: &[Agent], base: &str) -> String {
    let mut id = base.to_string();
    let mut n = 2;
    while agents.iter().any(|a| a.id == id) {
        id = format!("{}-{}", base, n);
        n += 1;
    }
    id
}

fn unique_group_id(groups: &[Group], base: &str) -> String {
    let mut id = base.to_string();
    let mut n = 2;
    while groups.iter().any(|g| g.id == id) {
        id = format!("{}-{}", base, n);
        n += 1;
    }
    id
}

/// Current time as epoch millis.
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Resolve a project's agent command and launch it in a new terminal.
fn resolve_and_launch(cfg: &config::Config, project_id: &str) -> Result<(), String> {
    let project = cfg
        .projects
        .iter()
        .find(|p| p.id == project_id)
        .ok_or_else(|| format!("project not found: {}", project_id))?;
    // Resolve: built-in agent > custom agent > raw agent id.
    let (command, agent_name) = config::builtin_agents()
        .iter()
        .find(|a| a.id == project.agent)
        .map(|a| (a.command.clone(), a.name.clone()))
        .or_else(|| {
            cfg.agents
                .iter()
                .find(|a| a.id == project.agent)
                .map(|a| (a.command.clone(), a.name.clone()))
        })
        .unwrap_or_else(|| (project.agent.clone(), project.agent.clone()));
    // Window/tab title: "project — agent" so the user can identify the
    // project even after the agent changes its own title in the TUI.
    let title = format!("{} — {}", project.name, agent_name);
    let term = terminal::default_terminal();
    term.launch(&title, &project.path, &command)
}

// ---- projects ----

#[tauri::command]
pub fn list_projects() -> Vec<Project> {
    config::load().projects
}

#[tauri::command]
pub fn add_project(name: String, path: String, agent: String) -> Result<Project, String> {
    if name.trim().is_empty() {
        return Err("name is required".into());
    }
    if path.trim().is_empty() {
        return Err("path is required".into());
    }
    let mut cfg = config::load();
    let id = unique_project_id(&cfg.projects, &config::slugify(&name));
    let project = Project {
        id,
        name: name.trim().to_string(),
        path: path.trim().to_string(),
        agent,
        legacy_context: None,
        created_at: now(),
        last_launched: None,
        launch_count: 0,
    };
    cfg.projects.push(project.clone());
    config::save(&cfg)?;
    Ok(project)
}

#[tauri::command]
pub fn remove_project(id: String) -> Result<(), String> {
    let mut cfg = config::load();
    cfg.projects.retain(|p| p.id != id);
    config::save(&cfg)
}

#[tauri::command]
pub fn launch_project(id: String) -> Result<(), String> {
    let mut cfg = config::load();
    if let Some(p) = cfg.projects.iter_mut().find(|p| p.id == id) {
        p.last_launched = Some(now());
        p.launch_count += 1;
    }
    config::save(&cfg)?;
    resolve_and_launch(&cfg, &id)
}

/// Persist a manual ordering of projects by id. Ids not present in
/// `ordered_ids` are appended in their previous relative order.
#[tauri::command]
pub fn reorder_projects(ordered_ids: Vec<String>) -> Result<(), String> {
    let mut cfg = config::load();
    let mut by_id: std::collections::HashMap<String, config::Project> =
        cfg.projects.into_iter().map(|p| (p.id.clone(), p)).collect();
    let mut ordered: Vec<config::Project> = Vec::with_capacity(ordered_ids.len());
    for id in &ordered_ids {
        if let Some(p) = by_id.remove(id) {
            ordered.push(p);
        }
    }
    for p in by_id.into_values() {
        ordered.push(p);
    }
    cfg.projects = ordered;
    config::save(&cfg)
}

// ---- context (v1: AGENTS.md is canonical) ----

const AGENTS_FILE: &str = "AGENTS.md";
const CLAUDE_FILE: &str = "CLAUDE.md";
const CLAUDE_POINTER: &str = "@AGENTS.md\n";
const MERGE_SENTINEL: &str = "<!-- termana-merge-block";
const CLAUDE_MERGE_MARKER: &str = "<!-- termana-merge-block: from CLAUDE.md -->";
const LEGACY_MERGE_MARKER: &str = "<!-- termana-merge-block: from legacy termana config -->";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileKind {
    Absent,
    Empty,
    Content,
    Symlink,
}

#[derive(Clone, Debug)]
struct TextFile {
    kind: FileKind,
    content: String,
    revision: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadContextResult {
    content: String,
    source: String,
    agents_revision: String,
    claude_revision: String,
    legacy_revision: String,
    requires_claude_conversion: bool,
    has_legacy_context: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveContextResult {
    claude_action: String,
    claude_error: Option<String>,
    has_merge_block: bool,
    legacy_migrated: bool,
    legacy_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextStatus {
    agents_exists: bool,
    agents_has_content: bool,
    claude_state: String,
    has_merge_block: bool,
    divergent: bool,
    has_legacy_context: bool,
}

fn stable_revision(label: &str, bytes: &[u8]) -> String {
    // Stable FNV-1a is sufficient for an optimistic concurrency token.
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in label.as_bytes().iter().chain(bytes.iter()) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{label}:{hash:016x}")
}

fn legacy_revision(value: &Option<String>) -> String {
    match value {
        Some(content) => stable_revision("legacy", content.as_bytes()),
        None => stable_revision("none", &[]),
    }
}

fn read_text_file(path: &Path, preserve_symlink: bool) -> Result<TextFile, String> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(TextFile {
                kind: FileKind::Absent,
                content: String::new(),
                revision: stable_revision("absent", &[]),
            });
        }
        Err(error) => return Err(format!("failed to inspect {}: {error}", path.display())),
    };
    if metadata.file_type().is_symlink() && preserve_symlink {
        let target = std::fs::read_link(path)
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        return Ok(TextFile {
            kind: FileKind::Symlink,
            content: String::new(),
            revision: stable_revision("symlink", target.as_bytes()),
        });
    }
    if !metadata.file_type().is_file() && !metadata.file_type().is_symlink() {
        return Err(format!("{} is not a file", path.display()));
    }
    let content = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let kind = if content.trim().is_empty() {
        FileKind::Empty
    } else {
        FileKind::Content
    };
    let label = if kind == FileKind::Empty {
        "empty"
    } else {
        "content"
    };
    Ok(TextFile {
        kind,
        revision: stable_revision(label, content.as_bytes()),
        content,
    })
}

fn normalized_content(content: &str) -> String {
    content.replace("\r\n", "\n").trim().to_string()
}

fn equivalent(left: &str, right: &str) -> bool {
    normalized_content(left) == normalized_content(right)
}

fn claude_is_linked(file: &TextFile) -> bool {
    file.kind == FileKind::Symlink || file.content.contains("@AGENTS.md")
}

fn append_merge_block(content: &mut String, marker: &str, source: &str, incoming: &str) {
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&format!(
        "\n---\n\n{marker}\n\n> The content below came from {source} and differs from AGENTS.md. Merge unique information into the canonical content, resolve conflicts, then remove this entire block.\n>\n> AI agent: only perform this merge when the user explicitly asks you to organize AGENTS.md.\n\n{incoming}"
    ));
}

fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    let target = match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => std::fs::canonicalize(path)
            .map_err(|error| format!("failed to resolve {}: {error}", path.display()))?,
        Ok(_) => path.to_path_buf(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => path.to_path_buf(),
        Err(error) => return Err(format!("failed to inspect {}: {error}", path.display())),
    };
    let parent = target
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", target.display()))?;
    let mut temp = tempfile::Builder::new()
        .prefix(".termana-context-")
        .tempfile_in(parent)
        .map_err(|error| format!("failed to create temporary file: {error}"))?;
    if let Ok(metadata) = std::fs::metadata(&target) {
        temp.as_file()
            .set_permissions(metadata.permissions())
            .map_err(|error| format!("failed to preserve permissions: {error}"))?;
    }
    temp.write_all(content.as_bytes())
        .map_err(|error| format!("failed to write temporary file: {error}"))?;
    temp.as_file()
        .sync_all()
        .map_err(|error| format!("failed to flush temporary file: {error}"))?;
    temp.persist(&target)
        .map_err(|error| format!("failed to replace {}: {}", target.display(), error.error))?;
    Ok(())
}

fn project_context_paths(project: &Project) -> (PathBuf, PathBuf) {
    let root = Path::new(&project.path);
    (root.join(AGENTS_FILE), root.join(CLAUDE_FILE))
}

fn read_context_for_project(project: &Project) -> Result<ReadContextResult, String> {
    let (agents_path, claude_path) = project_context_paths(project);
    let agents = read_text_file(&agents_path, false)?;
    let claude = read_text_file(&claude_path, true)?;
    let legacy = project
        .legacy_context
        .as_ref()
        .filter(|content| !content.trim().is_empty());
    let claude_independent = claude.kind == FileKind::Content && !claude_is_linked(&claude);

    let (mut content, mut source) = if agents.kind == FileKind::Content {
        (agents.content.clone(), "agents")
    } else if claude_independent {
        (claude.content.clone(), "claude")
    } else if let Some(legacy) = legacy {
        (legacy.clone(), "legacy")
    } else {
        (String::new(), "empty")
    };

    if agents.kind == FileKind::Content
        && claude_independent
        && !equivalent(&agents.content, &claude.content)
        && !content.contains(CLAUDE_MERGE_MARKER)
    {
        append_merge_block(
            &mut content,
            CLAUDE_MERGE_MARKER,
            CLAUDE_FILE,
            &claude.content,
        );
        source = "merge";
    }

    if let Some(legacy) = legacy {
        let already_represented = equivalent(&agents.content, legacy)
            || (claude_independent && equivalent(&claude.content, legacy));
        if !already_represented && source != "legacy" && !content.contains(LEGACY_MERGE_MARKER) {
            append_merge_block(
                &mut content,
                LEGACY_MERGE_MARKER,
                "termana's legacy config",
                legacy,
            );
            source = "merge";
        }
    }

    Ok(ReadContextResult {
        content,
        source: source.to_string(),
        agents_revision: agents.revision,
        claude_revision: claude.revision,
        legacy_revision: legacy_revision(&project.legacy_context),
        requires_claude_conversion: claude_independent,
        has_legacy_context: legacy.is_some(),
    })
}

#[derive(Debug)]
struct ContextFileSave {
    claude_action: String,
    claude_error: Option<String>,
}

fn save_context_files(
    project: &Project,
    content: &str,
    expected_agents_revision: &str,
    expected_claude_revision: &str,
    convert_claude: bool,
) -> Result<ContextFileSave, String> {
    let (agents_path, claude_path) = project_context_paths(project);
    let agents = read_text_file(&agents_path, false)?;
    let claude = read_text_file(&claude_path, true)?;
    if agents.revision != expected_agents_revision || claude.revision != expected_claude_revision {
        return Err("Context files changed after the editor was opened. Reopen the editor and merge the latest content.".into());
    }

    atomic_write(&agents_path, content)
        .map_err(|error| format!("failed to save {AGENTS_FILE}: {error}"))?;

    let (claude_action, claude_error) = match claude.kind {
        FileKind::Symlink => ("symlink-skip".to_string(), None),
        FileKind::Absent | FileKind::Empty => match atomic_write(&claude_path, CLAUDE_POINTER) {
            Ok(()) => ("created".to_string(), None),
            Err(error) => ("failed".to_string(), Some(error)),
        },
        FileKind::Content if claude_is_linked(&claude) => ("already-linked".to_string(), None),
        FileKind::Content if !convert_claude => ("independent-kept".to_string(), None),
        FileKind::Content => {
            let current = read_text_file(&claude_path, true)?;
            if current.revision != expected_claude_revision {
                (
                    "failed".to_string(),
                    Some("CLAUDE.md changed during save and was not overwritten".to_string()),
                )
            } else {
                match atomic_write(&claude_path, CLAUDE_POINTER) {
                    Ok(()) => ("converted".to_string(), None),
                    Err(error) => ("failed".to_string(), Some(error)),
                }
            }
        }
    };

    Ok(ContextFileSave {
        claude_action,
        claude_error,
    })
}

#[tauri::command]
pub fn read_context(project_id: String) -> Result<ReadContextResult, String> {
    let cfg = config::load();
    let project = cfg
        .projects
        .iter()
        .find(|project| project.id == project_id)
        .ok_or_else(|| format!("project not found: {project_id}"))?;
    read_context_for_project(project)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn save_context(
    project_id: String,
    content: String,
    expected_agents_revision: String,
    expected_claude_revision: String,
    expected_legacy_revision: String,
    convert_claude: bool,
    migrate_legacy: bool,
) -> Result<SaveContextResult, String> {
    let cfg = config::load();
    let project = cfg
        .projects
        .iter()
        .find(|project| project.id == project_id)
        .cloned()
        .ok_or_else(|| format!("project not found: {project_id}"))?;
    if legacy_revision(&project.legacy_context) != expected_legacy_revision {
        return Err("Context files changed after the editor was opened. Reopen the editor and merge the latest content.".into());
    }
    let file_save = save_context_files(
        &project,
        &content,
        &expected_agents_revision,
        &expected_claude_revision,
        convert_claude,
    )?;

    let mut legacy_migrated = false;
    let mut legacy_error = None;
    if migrate_legacy && project.legacy_context.is_some() {
        let mut latest = config::load();
        if let Some(target) = latest
            .projects
            .iter_mut()
            .find(|item| item.id == project_id)
        {
            if legacy_revision(&target.legacy_context) == expected_legacy_revision {
                target.legacy_context = None;
                match config::save(&latest) {
                    Ok(()) => legacy_migrated = true,
                    Err(error) => legacy_error = Some(error),
                }
            } else {
                legacy_error =
                    Some("legacy context changed during save and was retained".to_string());
            }
        }
    }

    Ok(SaveContextResult {
        claude_action: file_save.claude_action,
        claude_error: file_save.claude_error,
        has_merge_block: content.contains(MERGE_SENTINEL),
        legacy_migrated,
        legacy_error,
    })
}

fn context_status_for_project(project: &Project) -> Result<ContextStatus, String> {
    let (agents_path, claude_path) = project_context_paths(project);
    let agents = read_text_file(&agents_path, false)?;
    let claude = read_text_file(&claude_path, true)?;
    let claude_state = match claude.kind {
        FileKind::Absent => "absent",
        FileKind::Empty => "empty",
        FileKind::Symlink => "symlink",
        FileKind::Content if claude_is_linked(&claude) => "linked",
        FileKind::Content => "independent",
    };
    let divergent = agents.kind == FileKind::Content
        && claude_state == "independent"
        && !equivalent(&agents.content, &claude.content);
    Ok(ContextStatus {
        agents_exists: agents.kind != FileKind::Absent,
        agents_has_content: agents.kind == FileKind::Content,
        claude_state: claude_state.to_string(),
        has_merge_block: agents.content.contains(MERGE_SENTINEL),
        divergent,
        has_legacy_context: project
            .legacy_context
            .as_ref()
            .is_some_and(|content| !content.trim().is_empty()),
    })
}

#[tauri::command]
pub fn context_status(project_id: String) -> Result<ContextStatus, String> {
    let cfg = config::load();
    let project = cfg
        .projects
        .iter()
        .find(|project| project.id == project_id)
        .ok_or_else(|| format!("project not found: {project_id}"))?;
    context_status_for_project(project)
}

/// Whether a path exists and is a directory (used to validate the project
/// path in the Add Project form before allowing it to be saved).
#[tauri::command]
pub fn path_exists(path: String) -> bool {
    Path::new(&path).is_dir()
}

// ---- agents ----

#[tauri::command]
pub fn list_agents() -> Vec<AgentInfo> {
    let builtins = config::builtin_agents();
    let builtin_ids: std::collections::HashSet<&str> =
        builtins.iter().map(|a| a.id.as_str()).collect();
    let custom: Vec<Agent> = config::load()
        .agents
        .into_iter()
        .filter(|a| !builtin_ids.contains(a.id.as_str()))
        .collect();

    let all: Vec<(Agent, bool)> = builtins
        .into_iter()
        .map(|a| (a, true))
        .chain(custom.into_iter().map(|a| (a, false)))
        .collect();

    let commands: Vec<&str> = all.iter().map(|(a, _)| a.command.as_str()).collect();
    let installed = agent::installed_status(&commands);
    all.iter()
        .enumerate()
        .map(|(i, (a, built_in))| AgentInfo {
            id: a.id.clone(),
            name: a.name.clone(),
            command: a.command.clone(),
            installed: installed.get(i).copied().unwrap_or(false),
            built_in: *built_in,
        })
        .collect()
}

#[tauri::command]
pub fn add_agent(name: String, command: String) -> Result<Agent, String> {
    if name.trim().is_empty() {
        return Err("name is required".into());
    }
    if command.trim().is_empty() {
        return Err("command is required".into());
    }
    let id_base = config::slugify(&name);
    if config::is_builtin_id(&id_base) {
        return Err("a built-in agent with this name already exists".into());
    }
    let mut cfg = config::load();
    let id = unique_agent_id(&cfg.agents, &id_base);
    let new_agent = Agent {
        id,
        name: name.trim().to_string(),
        command: command.trim().to_string(),
    };
    cfg.agents.push(new_agent.clone());
    config::save(&cfg)?;
    Ok(new_agent)
}

#[tauri::command]
pub fn update_agent(id: String, name: String, command: String) -> Result<(), String> {
    if config::is_builtin_id(&id) {
        return Err("built-in agents cannot be modified".into());
    }
    if name.trim().is_empty() {
        return Err("name is required".into());
    }
    if command.trim().is_empty() {
        return Err("command is required".into());
    }
    let mut cfg = config::load();
    let target = cfg
        .agents
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or_else(|| format!("agent not found: {}", id))?;
    target.name = name.trim().to_string();
    target.command = command.trim().to_string();
    config::save(&cfg)
}

#[tauri::command]
pub fn remove_agent(id: String) -> Result<(), String> {
    if config::is_builtin_id(&id) {
        return Err("built-in agents cannot be deleted".into());
    }
    let mut cfg = config::load();
    cfg.agents.retain(|a| a.id != id);
    config::save(&cfg)
}

// ---- groups ----

#[tauri::command]
pub fn list_groups() -> Vec<Group> {
    config::load().groups
}

#[tauri::command]
pub fn add_group(name: String, project_ids: Vec<String>) -> Result<Group, String> {
    if name.trim().is_empty() {
        return Err("name is required".into());
    }
    let mut cfg = config::load();
    let id = unique_group_id(&cfg.groups, &config::slugify(&name));
    let group = Group {
        id,
        name: name.trim().to_string(),
        project_ids,
    };
    cfg.groups.push(group.clone());
    config::save(&cfg)?;
    Ok(group)
}

#[tauri::command]
pub fn update_group(id: String, name: String, project_ids: Vec<String>) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("name is required".into());
    }
    let mut cfg = config::load();
    let target = cfg
        .groups
        .iter_mut()
        .find(|g| g.id == id)
        .ok_or_else(|| format!("group not found: {}", id))?;
    target.name = name.trim().to_string();
    target.project_ids = project_ids;
    config::save(&cfg)
}

#[tauri::command]
pub fn remove_group(id: String) -> Result<(), String> {
    let mut cfg = config::load();
    cfg.groups.retain(|g| g.id != id);
    config::save(&cfg)
}

/// Launch every project in a group, each in its own terminal window.
#[tauri::command]
pub fn launch_group(id: String) -> Result<(), String> {
    let cfg = config::load();
    let group = cfg
        .groups
        .iter()
        .find(|g| g.id == id)
        .ok_or_else(|| format!("group not found: {}", id))?;
    let mut errors: Vec<String> = Vec::new();
    for pid in &group.project_ids {
        if let Err(e) = resolve_and_launch(&cfg, pid) {
            errors.push(format!("{}: {}", pid, e));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "some projects failed to launch: {}",
            errors.join("; ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn project(root: &TempDir, legacy_context: Option<&str>) -> Project {
        Project {
            id: "test".to_string(),
            name: "Test".to_string(),
            path: root.path().to_string_lossy().into_owned(),
            agent: "codex".to_string(),
            legacy_context: legacy_context.map(str::to_string),
            created_at: 0,
            last_launched: None,
            launch_count: 0,
        }
    }

    fn write(root: &TempDir, name: &str, content: &str) {
        std::fs::write(root.path().join(name), content).unwrap();
    }

    #[test]
    fn reads_the_six_context_states() {
        let root = tempfile::tempdir().unwrap();
        let project = project(&root, None);

        let result = read_context_for_project(&project).unwrap();
        assert_eq!(result.source, "empty");

        write(&root, CLAUDE_FILE, CLAUDE_POINTER);
        let result = read_context_for_project(&project).unwrap();
        assert_eq!(result.source, "empty");
        assert!(!result.requires_claude_conversion);

        write(&root, CLAUDE_FILE, "Claude only");
        let result = read_context_for_project(&project).unwrap();
        assert_eq!(result.source, "claude");
        assert_eq!(result.content, "Claude only");
        assert!(result.requires_claude_conversion);

        std::fs::remove_file(root.path().join(CLAUDE_FILE)).unwrap();
        write(&root, AGENTS_FILE, "Canonical");
        let result = read_context_for_project(&project).unwrap();
        assert_eq!(result.source, "agents");

        write(&root, CLAUDE_FILE, CLAUDE_POINTER);
        let result = read_context_for_project(&project).unwrap();
        assert_eq!(result.source, "agents");
        assert!(!result.requires_claude_conversion);

        write(&root, CLAUDE_FILE, "Different Claude context");
        let result = read_context_for_project(&project).unwrap();
        assert_eq!(result.source, "merge");
        assert!(result.content.contains("Canonical"));
        assert!(result.content.contains("Different Claude context"));
        assert_eq!(result.content.matches(CLAUDE_MERGE_MARKER).count(), 1);
    }

    #[test]
    fn includes_legacy_context_without_losing_file_content() {
        let root = tempfile::tempdir().unwrap();
        write(&root, AGENTS_FILE, "Current file context");
        let project = project(&root, Some("Unsynced legacy context"));

        let result = read_context_for_project(&project).unwrap();

        assert_eq!(result.source, "merge");
        assert!(result.content.contains("Current file context"));
        assert!(result.content.contains("Unsynced legacy context"));
        assert!(result.has_legacy_context);
    }

    #[test]
    fn creates_pointer_for_an_absent_claude_file() {
        let root = tempfile::tempdir().unwrap();
        let project = project(&root, None);
        let snapshot = read_context_for_project(&project).unwrap();

        let saved = save_context_files(
            &project,
            "Canonical",
            &snapshot.agents_revision,
            &snapshot.claude_revision,
            false,
        )
        .unwrap();

        assert_eq!(saved.claude_action, "created");
        assert_eq!(
            std::fs::read_to_string(root.path().join(AGENTS_FILE)).unwrap(),
            "Canonical"
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join(CLAUDE_FILE)).unwrap(),
            CLAUDE_POINTER
        );
    }

    #[test]
    fn independent_claude_file_requires_conversion_consent() {
        let root = tempfile::tempdir().unwrap();
        write(&root, CLAUDE_FILE, "Keep me");
        let project = project(&root, None);
        let snapshot = read_context_for_project(&project).unwrap();

        let kept = save_context_files(
            &project,
            "Canonical",
            &snapshot.agents_revision,
            &snapshot.claude_revision,
            false,
        )
        .unwrap();
        assert_eq!(kept.claude_action, "independent-kept");
        assert_eq!(
            std::fs::read_to_string(root.path().join(CLAUDE_FILE)).unwrap(),
            "Keep me"
        );

        let snapshot = read_context_for_project(&project).unwrap();
        let converted = save_context_files(
            &project,
            &snapshot.content,
            &snapshot.agents_revision,
            &snapshot.claude_revision,
            true,
        )
        .unwrap();
        assert_eq!(converted.claude_action, "converted");
        assert_eq!(
            std::fs::read_to_string(root.path().join(CLAUDE_FILE)).unwrap(),
            CLAUDE_POINTER
        );
    }

    #[test]
    fn rejects_stale_editor_revisions_before_writing() {
        let root = tempfile::tempdir().unwrap();
        write(&root, AGENTS_FILE, "Initial");
        let project = project(&root, None);
        let snapshot = read_context_for_project(&project).unwrap();
        write(&root, AGENTS_FILE, "External edit");

        let error = save_context_files(
            &project,
            "Editor edit",
            &snapshot.agents_revision,
            &snapshot.claude_revision,
            false,
        )
        .unwrap_err();

        assert!(error.contains("changed after the editor was opened"));
        assert_eq!(
            std::fs::read_to_string(root.path().join(AGENTS_FILE)).unwrap(),
            "External edit"
        );
    }

    #[cfg(unix)]
    #[test]
    fn never_replaces_a_symlinked_claude_file() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        write(&root, AGENTS_FILE, "Initial");
        write(&root, "CLAUDE-target.md", "Target content");
        symlink("CLAUDE-target.md", root.path().join(CLAUDE_FILE)).unwrap();
        let project = project(&root, None);
        let snapshot = read_context_for_project(&project).unwrap();

        let saved = save_context_files(
            &project,
            "Updated",
            &snapshot.agents_revision,
            &snapshot.claude_revision,
            true,
        )
        .unwrap();

        assert_eq!(saved.claude_action, "symlink-skip");
        assert_eq!(
            std::fs::read_to_string(root.path().join("CLAUDE-target.md")).unwrap(),
            "Target content"
        );
        assert!(std::fs::symlink_metadata(root.path().join(CLAUDE_FILE))
            .unwrap()
            .file_type()
            .is_symlink());
    }
}
