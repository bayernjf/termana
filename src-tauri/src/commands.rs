use crate::adapters::{agent, terminal};
use crate::config::{self, Agent, Project};
use serde::Serialize;

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
    let cfg = config::load();
    let project = cfg
        .projects
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("project not found: {}", id))?;
    // Resolve: built-in agent > custom agent > raw agent id.
    let builtins = config::builtin_agents();
    let command = builtins
        .iter()
        .find(|a| a.id == project.agent)
        .map(|a| a.command.clone())
        .or_else(|| cfg.agents.iter().find(|a| a.id == project.agent).map(|a| a.command.clone()))
        .unwrap_or_else(|| project.agent.clone());
    let term = terminal::default_terminal();
    term.launch(&project.path, &command)
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
