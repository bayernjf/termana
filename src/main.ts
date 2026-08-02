import { invoke } from "@tauri-apps/api/core";

interface Project {
  id: string;
  name: string;
  path: string;
  agent: string;
  agentCommand: string | null;
}

interface AgentInfo {
  id: string;
  name: string;
  command: string;
  installed: boolean;
}

let projects: Project[] = [];
let agents: AgentInfo[] = [];
let editingAgentId: string | null = null;

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c] as string)
  );
}

function renderProjects() {
  const container = document.getElementById("projects")!;
  if (projects.length === 0) {
    container.innerHTML = `<div class="empty">No projects yet. Click “+ Add” to create one.</div>`;
    return;
  }
  container.innerHTML = projects
    .map(
      (p) => `
    <div class="card" data-id="${escapeHtml(p.id)}">
      <div class="card-head">
        <div class="card-name">${escapeHtml(p.name)}</div>
        <span class="chip">${escapeHtml(p.agent)}</span>
        <button class="icon-btn delete" data-id="${escapeHtml(p.id)}" title="Remove project">✕</button>
      </div>
      <div class="card-path">${escapeHtml(p.path)}</div>
      <div class="card-launch">Launch ▸</div>
    </div>`
    )
    .join("");
}

function renderAgents() {
  const container = document.getElementById("agents-list")!;
  if (agents.length === 0) {
    container.innerHTML = `<div class="empty">No agents. Click “+ Add” to add one.</div>`;
    return;
  }
  container.innerHTML = agents
    .map(
      (a) => `
    <div class="agent-row" data-id="${escapeHtml(a.id)}">
      <span class="status-dot ${a.installed ? "on" : ""}"></span>
      <span class="agent-name">${escapeHtml(a.name)}</span>
      <span class="agent-cmd" title="${escapeHtml(a.command)}">${escapeHtml(a.command)}</span>
      <span class="agent-status ${a.installed ? "on" : ""}">${a.installed ? "installed" : "missing"}</span>
      <button class="icon-btn edit" data-id="${escapeHtml(a.id)}" title="Edit">✎</button>
      <button class="icon-btn delete" data-id="${escapeHtml(a.id)}" title="Remove agent">✕</button>
    </div>`
    )
    .join("");
}

function renderAgentOptions() {
  const select = document.getElementById("agent") as HTMLSelectElement;
  const sorted = [...agents].sort((a, b) => Number(b.installed) - Number(a.installed));
  select.innerHTML = sorted
    .map((a) => {
      const label = a.installed ? a.name : `${a.name} (not installed)`;
      const disabled = a.installed ? "" : "disabled";
      return `<option value="${escapeHtml(a.id)}" ${disabled}>${escapeHtml(label)}</option>`;
    })
    .join("");
}

function resetAgentForm() {
  editingAgentId = null;
  (document.getElementById("agent-name") as HTMLInputElement).value = "";
  (document.getElementById("agent-command") as HTMLInputElement).value = "";
  (document.getElementById("agent-submit") as HTMLButtonElement).textContent = "Add agent";
  document.getElementById("agent-cancel")!.classList.add("hidden");
}

async function refreshAgents() {
  agents = await invoke<AgentInfo[]>("list_agents");
  renderAgents();
  renderAgentOptions();
}

async function refreshProjects() {
  projects = await invoke<Project[]>("list_projects");
  renderProjects();
}

window.addEventListener("DOMContentLoaded", async () => {
  await refreshAgents();
  await refreshProjects();

  // toggle add forms
  document.getElementById("toggle-add-project")!.addEventListener("click", () => {
    document.getElementById("add-form")!.classList.toggle("hidden");
  });
  document.getElementById("toggle-add-agent")!.addEventListener("click", () => {
    resetAgentForm();
    document.getElementById("agent-form")!.classList.toggle("hidden");
  });

  // project card: click anywhere = launch, delete button = remove
  document.getElementById("projects")!.addEventListener("click", async (e) => {
    const target = e.target as HTMLElement;
    const card = target.closest(".card") as HTMLElement | null;
    if (!card) return;
    const id = card.getAttribute("data-id");
    if (!id) return;
    if (target.classList.contains("delete")) {
      if (!confirm("Remove this project?")) return;
      await invoke("remove_project", { id });
      await refreshProjects();
      return;
    }
    try {
      await invoke("launch_project", { id });
    } catch (err) {
      alert("Launch failed: " + err);
    }
  });

  document.getElementById("add-form")!.addEventListener("submit", async (e) => {
    e.preventDefault();
    const name = (document.getElementById("name") as HTMLInputElement).value;
    const path = (document.getElementById("path") as HTMLInputElement).value;
    const agent = (document.getElementById("agent") as HTMLSelectElement).value;
    const command = (document.getElementById("command") as HTMLInputElement).value;
    try {
      await invoke("add_project", { name, path, agent, command: command || null });
      (e.target as HTMLFormElement).reset();
      await refreshProjects();
    } catch (err) {
      alert("Add failed: " + err);
    }
  });

  // agent row: edit / delete
  document.getElementById("agents-list")!.addEventListener("click", async (e) => {
    const target = e.target as HTMLElement;
    const id = target.getAttribute("data-id");
    if (!id) return;
    if (target.classList.contains("delete")) {
      if (!confirm("Remove this agent?")) return;
      await invoke("remove_agent", { id });
      await refreshAgents();
    } else if (target.classList.contains("edit")) {
      const a = agents.find((x) => x.id === id);
      if (!a) return;
      editingAgentId = id;
      (document.getElementById("agent-name") as HTMLInputElement).value = a.name;
      (document.getElementById("agent-command") as HTMLInputElement).value = a.command;
      (document.getElementById("agent-submit") as HTMLButtonElement).textContent = "Update agent";
      document.getElementById("agent-cancel")!.classList.remove("hidden");
      document.getElementById("agent-form")!.classList.remove("hidden");
    }
  });

  document.getElementById("agent-form")!.addEventListener("submit", async (e) => {
    e.preventDefault();
    const name = (document.getElementById("agent-name") as HTMLInputElement).value;
    const command = (document.getElementById("agent-command") as HTMLInputElement).value;
    try {
      if (editingAgentId) {
        await invoke("update_agent", { id: editingAgentId, name, command });
        resetAgentForm();
        document.getElementById("agent-form")!.classList.add("hidden");
      } else {
        await invoke("add_agent", { name, command });
        (e.target as HTMLFormElement).reset();
      }
      await refreshAgents();
    } catch (err) {
      alert("Agent save failed: " + err);
    }
  });

  document.getElementById("agent-cancel")!.addEventListener("click", () => {
    resetAgentForm();
    document.getElementById("agent-form")!.classList.add("hidden");
  });
});
