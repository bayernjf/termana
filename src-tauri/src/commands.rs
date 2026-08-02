use crate::adapters::{agent, terminal};
use crate::config::{self, Agent, Project};
use serde::Serialize;

#[derive(Serialize)]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    pub command: String,
    pub installed: bool,
}

fn slugify(s: &str) -> String {
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
pub fn add_project(
    name: String,
    path: String,
    agent: String,
    command: Option<String>,
) -> Result<Project, String> {
    if name.trim().is_empty() {
        return Err("name is required".into());
    }
    if path.trim().is_empty() {
        return Err("path is required".into());
    }
    let mut cfg = config::load();
    let id = unique_project_id(&cfg.projects, &slugify(&name));
    let project = Project {
        id,
        name: name.trim().to_string(),
        path: path.trim().to_string(),
        agent,
        agent_command: command.filter(|c| !c.trim().is_empty()),
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
    let agents = if cfg.agents.is_empty() {
        config::default_agents()
    } else {
        cfg.agents.clone()
    };
    // Resolve: explicit override > configured agent's command > raw agent id.
    let command = project
        .agent_command
        .clone()
        .or_else(|| agents.iter().find(|a| a.id == project.agent).map(|a| a.command.clone()))
        .unwrap_or_else(|| project.agent.clone());
    let term = terminal::default_terminal();
    term.launch(&project.path, &command)
}

// ---- agents ----

#[tauri::command]
pub fn list_agents() -> Vec<AgentInfo> {
    let agents = config::load_agents_or_seed();
    let commands: Vec<&str> = agents.iter().map(|a| a.command.as_str()).collect();
    let installed = agent::installed_status(&commands);
    agents
        .iter()
        .enumerate()
        .map(|(i, a)| AgentInfo {
            id: a.id.clone(),
            name: a.name.clone(),
            command: a.command.clone(),
            installed: installed.get(i).copied().unwrap_or(false),
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
    let mut cfg = config::load();
    if cfg.agents.is_empty() {
        cfg.agents = config::default_agents();
    }
    let id = unique_agent_id(&cfg.agents, &slugify(&name));
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
    let mut cfg = config::load();
    cfg.agents.retain(|a| a.id != id);
    config::save(&cfg)
}
