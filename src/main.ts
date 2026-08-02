import { invoke } from "@tauri-apps/api/core";
import { open, confirm, message } from "@tauri-apps/plugin-dialog";

interface Project {
  id: string;
  name: string;
  path: string;
  agent: string;
}

interface AgentInfo {
  id: string;
  name: string;
  command: string;
  installed: boolean;
  builtIn: boolean;
}

let projects: Project[] = [];
let agents: AgentInfo[] = [];
let editingAgentId: string | null = null;
let pathValid = false;

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
      <button class="card-launch">Launch ▸</button>
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
    .map((a) => {
      const kind = a.builtIn ? `<span class="tag-builtIn">built-in</span>` : "";
      const actions = a.builtIn
        ? ""
        : `<button class="icon-btn edit" data-id="${escapeHtml(a.id)}" title="Edit">✎</button>
           <button class="icon-btn delete" data-id="${escapeHtml(a.id)}" title="Remove agent">✕</button>`;
      return `
    <div class="agent-row" data-id="${escapeHtml(a.id)}">
      <span class="status-dot ${a.installed ? "on" : ""}"></span>
      <span class="agent-name">${escapeHtml(a.name)}</span>
      <span class="agent-cmd" title="${escapeHtml(a.command)}">${escapeHtml(a.command)}</span>
      <span class="agent-kind">${kind}</span>
      <span class="agent-status ${a.installed ? "on" : ""}">${a.installed ? "installed" : "missing"}</span>
      <span class="agent-actions">${actions}</span>
    </div>`;
    })
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

async function validatePath() {
  const input = document.getElementById("path") as HTMLInputElement;
  const msg = document.getElementById("path-msg")!;
  const addBtn = document.getElementById("add-project-btn") as HTMLButtonElement;
  const p = input.value.trim();
  if (p === "") {
    pathValid = false;
    input.classList.remove("invalid");
    msg.textContent = "";
    addBtn.disabled = true;
    return;
  }
  const exists = await invoke<boolean>("path_exists", { path: p });
  if (exists) {
    pathValid = true;
    input.classList.remove("invalid");
    msg.textContent = "";
    addBtn.disabled = false;
  } else {
    pathValid = false;
    input.classList.add("invalid");
    msg.textContent = "Path does not exist";
    addBtn.disabled = true;
  }
}

window.addEventListener("DOMContentLoaded", async () => {
  await refreshAgents();
  await refreshProjects();

  document.getElementById("toggle-add-project")!.addEventListener("click", () => {
    document.getElementById("add-form")!.classList.toggle("hidden");
  });
  document.getElementById("toggle-add-agent")!.addEventListener("click", () => {
    resetAgentForm();
    document.getElementById("agent-form")!.classList.toggle("hidden");
  });

  const pathInput = document.getElementById("path") as HTMLInputElement;
  const nameInput = document.getElementById("name") as HTMLInputElement;
  let nameTouched = false;
  let pathDebounce: ReturnType<typeof setTimeout> | undefined;

  nameInput.addEventListener("input", () => {
    nameTouched = nameInput.value.trim() !== "";
  });

  const fillNameFromPath = () => {
    if (nameTouched) return;
    const folderName = pathInput.value.split(/[\\/]/).filter(Boolean).pop() ?? "";
    nameInput.value = folderName;
  };

  const onPathChanged = async () => {
    fillNameFromPath();
    await validatePath();
  };

  document.getElementById("browse-btn")!.addEventListener("click", async () => {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string" && selected.length > 0) {
      pathInput.value = selected;
      await onPathChanged();
    }
  });

  const onPathChangedNow = () => {
    clearTimeout(pathDebounce);
    onPathChanged();
  };
  pathInput.addEventListener("input", () => {
    clearTimeout(pathDebounce);
    pathDebounce = setTimeout(onPathChanged, 300);
  });
  pathInput.addEventListener("blur", onPathChangedNow);
  pathInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      onPathChangedNow();
    }
  });

  document.getElementById("refresh-agents")!.addEventListener("click", async () => {
    const btn = document.getElementById("refresh-agents") as HTMLButtonElement;
    btn.disabled = true;
    btn.classList.add("spinning");
    try {
      await refreshAgents();
    } finally {
      btn.classList.remove("spinning");
      btn.disabled = false;
    }
  });

  // project card: ✕ = confirm-remove, Launch ▸ = direct launch,
  // card body = confirm-then-launch
  document.getElementById("projects")!.addEventListener("click", async (e) => {
    const target = e.target as HTMLElement;
    const card = target.closest(".card") as HTMLElement | null;
    if (!card) return;
    const id = card.getAttribute("data-id");
    if (!id) return;
    const launch = async () => {
      try {
        await invoke("launch_project", { id });
      } catch (err) {
        await message(String(err), { title: "Launch failed", kind: "error" });
      }
    };
    if (target.classList.contains("delete")) {
      const ok = await confirm("Remove this project?", { title: "Remove project", kind: "warning" });
      if (!ok) return;
      await invoke("remove_project", { id });
      await refreshProjects();
      return;
    }
    if (target.classList.contains("card-launch")) {
      await launch();
      return;
    }
    const p = projects.find((x) => x.id === id);
    const ok = await confirm(`Launch “${p?.name ?? "project"}”?`, { title: "Launch project" });
    if (!ok) return;
    await launch();
  });

  document.getElementById("add-form")!.addEventListener("submit", async (e) => {
    e.preventDefault();
    if (!pathValid) return;
    const name = (document.getElementById("name") as HTMLInputElement).value;
    const path = (document.getElementById("path") as HTMLInputElement).value;
    const agent = (document.getElementById("agent") as HTMLSelectElement).value;
    try {
      await invoke("add_project", { name, path, agent });
      (e.target as HTMLFormElement).reset();
      pathValid = false;
      (document.getElementById("add-project-btn") as HTMLButtonElement).disabled = true;
      await refreshProjects();
    } catch (err) {
      await message(String(err), { title: "Add failed", kind: "error" });
    }
  });

  // agent row: edit / delete
  document.getElementById("agents-list")!.addEventListener("click", async (e) => {
    const target = e.target as HTMLElement;
    const id = target.getAttribute("data-id");
    if (!id) return;
    if (target.classList.contains("delete")) {
      const ok = await confirm("Remove this agent?", { title: "Remove agent", kind: "warning" });
      if (!ok) return;
      try {
        await invoke("remove_agent", { id });
        await refreshAgents();
      } catch (err) {
        await message(String(err), { title: "Remove failed", kind: "error" });
      }
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
      await message(String(err), { title: "Save failed", kind: "error" });
    }
  });

  document.getElementById("agent-cancel")!.addEventListener("click", () => {
    resetAgentForm();
    document.getElementById("agent-form")!.classList.add("hidden");
  });
});
