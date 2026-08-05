import { invoke } from "@tauri-apps/api/core";
import { open, confirm, message } from "@tauri-apps/plugin-dialog";
import { marked } from "marked";

interface Project {
  id: string;
  name: string;
  path: string;
  agent: string;
  createdAt: number;
  lastLaunched: number | null;
  launchCount: number;
}

interface ContextStatus {
  agentsExists: boolean;
  agentsHasContent: boolean;
  claudeState: "absent" | "empty" | "linked" | "independent" | "symlink";
  hasMergeBlock: boolean;
  divergent: boolean;
  hasLegacyContext: boolean;
}

interface ReadContextResult {
  content: string;
  source: "agents" | "claude" | "legacy" | "empty" | "merge";
  agentsRevision: string;
  claudeRevision: string;
  legacyRevision: string;
  requiresClaudeConversion: boolean;
  hasLegacyContext: boolean;
}

interface SaveContextResult {
  claudeAction:
    | "created"
    | "already-linked"
    | "converted"
    | "independent-kept"
    | "failed"
    | "symlink-skip";
  claudeError: string | null;
  hasMergeBlock: boolean;
  legacyMigrated: boolean;
  legacyError: string | null;
}

interface AgentInfo {
  id: string;
  name: string;
  command: string;
  installed: boolean;
  builtIn: boolean;
}

interface Group {
  id: string;
  name: string;
  projectIds: string[];
}

let projects: Project[] = [];
let contextStatuses = new Map<string, ContextStatus>();
let agents: AgentInfo[] = [];
let groups: Group[] = [];
let editingAgentId: string | null = null;
let editingGroupId: string | null = null;
let pathValid = false;
let editingContextProjectId: string | null = null;
let editingContextSnapshot: ReadContextResult | null = null;
let contextDirty = false;
let lastSelectedAgentId: string | null = null;
let projectSortMode: string = localStorage.getItem("termana.projectSort") ?? "manual";
let projectAgentFilter: string = localStorage.getItem("termana.projectAgentFilter") ?? "all";
let activeTab: string = localStorage.getItem("termana.activeTab") ?? "projects";

const CONTEXT_FILES = "AGENTS.md (+ CLAUDE.md pointer)";

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c] as string)
  );
}

const previewRenderer = new marked.Renderer();
previewRenderer.html = ({ text }) => escapeHtml(text);
previewRenderer.link = ({ tokens }) => previewRenderer.parser.parseInline(tokens);
previewRenderer.image = ({ text }) => escapeHtml(`[image: ${text}]`);

function renderContextBadges(projectId: string): string {
  const status = contextStatuses.get(projectId);
  if (!status) return "";
  const badges: string[] = [];
  if (status.divergent) {
    badges.push('<span class="ctx-badge warning" title="AGENTS.md and CLAUDE.md differ">divergent</span>');
  }
  if (status.hasMergeBlock) {
    badges.push('<span class="ctx-badge warning" title="AGENTS.md has an unresolved merge block">merge</span>');
  }
  if (status.hasLegacyContext) {
    badges.push('<span class="ctx-badge migration" title="Legacy termana context will be migrated on save">migrate</span>');
  }
  if (status.agentsHasContent && badges.length === 0) {
    badges.push('<span class="ctx-badge" title="AGENTS.md has context">ctx</span>');
  }
  return badges.join("");
}

function sortProjects(list: Project[]): Project[] {
  const mode = projectSortMode;
  if (mode === "manual") return list;
  const copy = [...list];
  switch (mode) {
    case "name-asc":
      copy.sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
      break;
    case "name-desc":
      copy.sort((a, b) => b.name.localeCompare(a.name, undefined, { sensitivity: "base" }));
      break;
    case "added-asc":
      copy.sort((a, b) => a.createdAt - b.createdAt);
      break;
    case "added-desc":
      copy.sort((a, b) => b.createdAt - a.createdAt);
      break;
    case "recent":
      copy.sort((a, b) => (b.lastLaunched ?? 0) - (a.lastLaunched ?? 0));
      break;
  }
  return copy;
}

function agentLabel(agentId: string): string {
  return agents.find((a) => a.id === agentId)?.name ?? agentId;
}

function filterProjects(list: Project[]): Project[] {
  if (projectAgentFilter === "all") return list;
  return list.filter((p) => p.agent === projectAgentFilter);
}

function applyTab(tab: string) {
  activeTab = tab;
  document.querySelectorAll<HTMLButtonElement>("#main-tabs .tab-btn").forEach((btn) => {
    btn.classList.toggle("active", btn.dataset.tab === tab);
  });
  document.querySelectorAll<HTMLElement>(".panel").forEach((panel) => {
    panel.classList.toggle("hidden", panel.dataset.panel !== tab);
  });
}

// Counts per agent, most projects first; a filtered-on agent stays listed at zero.
function agentFilterCounts(): [string, number][] {
  const counts = new Map<string, number>();
  for (const p of projects) counts.set(p.agent, (counts.get(p.agent) ?? 0) + 1);
  if (projectAgentFilter !== "all" && !counts.has(projectAgentFilter)) {
    counts.set(projectAgentFilter, 0);
  }
  return [...counts.entries()].sort(
    (a, b) => b[1] - a[1] || agentLabel(a[0]).localeCompare(agentLabel(b[0]))
  );
}

function renderAgentFilter() {
  const btn = document.getElementById("agent-filter-btn");
  const menu = document.getElementById("agent-filter-menu");
  if (!btn || !menu) return;
  const active = projectAgentFilter !== "all";
  btn.textContent = `Agent：${active ? agentLabel(projectAgentFilter) : "全部"} ▾`;
  btn.classList.toggle("active", active);
  const entries: [string, number][] = [["all", projects.length], ...agentFilterCounts()];
  menu.innerHTML = entries
    .map(
      ([id, count]) => `
      <button type="button" class="filter-item ${id === projectAgentFilter ? "active" : ""}" data-agent="${escapeHtml(id)}">
        <span>${escapeHtml(id === "all" ? "全部" : agentLabel(id))}</span>
        <span class="filter-item-count">${count}</span>
      </button>`
    )
    .join("");
}

function renderProjects() {
  const container = document.getElementById("projects")!;
  const visible = filterProjects(projects);
  const countEl = document.getElementById("project-count");
  if (countEl) countEl.textContent = `项目数量：${visible.length}`;
  if (projects.length === 0) {
    container.innerHTML = `<div class="empty">No projects yet. Click “+ Add” to create one.</div>`;
    return;
  }
  if (visible.length === 0) {
    container.innerHTML = `<div class="empty">No projects for “${escapeHtml(agentLabel(projectAgentFilter))}”.</div>`;
    return;
  }
  // Reordering a filtered list would drop the hidden projects, so drag is manual-and-unfiltered only.
  const manual = projectSortMode === "manual" && projectAgentFilter === "all";
  // The single most recently launched project earns the “上次启动” flag.
  const latestId = projects
    .filter((p) => p.lastLaunched)
    .sort((a, b) => (b.lastLaunched ?? 0) - (a.lastLaunched ?? 0))[0]?.id;
  container.innerHTML = sortProjects(visible)
    .map(
      (p) => `
    <div class="card" data-id="${escapeHtml(p.id)}">
      <div class="card-head">
        ${manual ? '<span class="drag-handle" title="Drag to reorder">⠿</span>' : ""}
        <div class="card-name">${escapeHtml(p.name)}</div>
        <span class="chip">${escapeHtml(p.agent)}</span>
        ${renderContextBadges(p.id)}
        ${p.launchCount > 0 ? `<span class="chip count" title="Launch count">⟳ ${p.launchCount}</span>` : ""}
        ${p.id === latestId ? `<span class="ctx-badge last-launch" title="上次启动：${p.lastLaunched ? new Date(p.lastLaunched).toLocaleString() : ""}">上次启动</span>` : ""}
        <span class="card-actions">
          <button class="icon-btn context" data-id="${escapeHtml(p.id)}" title="Edit context">✎</button>
          <button class="icon-btn delete" data-id="${escapeHtml(p.id)}" title="Remove project">✕</button>
        </span>
      </div>
      <div class="card-path">${escapeHtml(p.path)}</div>
      <button class="card-launch">Launch ▸</button>
    </div>`
    )
    .join("");
}

function renderGroups() {
  const container = document.getElementById("groups")!;
  if (groups.length === 0) {
    container.innerHTML = `<div class="empty">No groups. Click “+ Add” to create one.</div>`;
    return;
  }
  container.innerHTML = groups
    .map((g) => {
      const memberNames = g.projectIds
        .map((pid) => projects.find((p) => p.id === pid)?.name ?? pid)
        .join(", ");
      const members = memberNames
        ? escapeHtml(memberNames)
        : `<span class="muted">no projects</span>`;
      return `
    <div class="card" data-id="${escapeHtml(g.id)}">
      <div class="card-head">
        <div class="card-name">${escapeHtml(g.name)}</div>
        <span class="chip">${g.projectIds.length} projects</span>
        <span class="card-actions">
          <button class="icon-btn edit" data-id="${escapeHtml(g.id)}" title="Edit group">✎</button>
          <button class="icon-btn delete" data-id="${escapeHtml(g.id)}" title="Remove group">✕</button>
        </span>
      </div>
      <div class="card-path">${members}</div>
      <button class="card-launch">Launch all ▸</button>
    </div>`;
    })
    .join("");
}

function renderGroupChecklist(selected: Set<string>) {
  const container = document.getElementById("group-projects")!;
  if (projects.length === 0) {
    container.innerHTML = `<div class="muted">No projects yet. Add projects first.</div>`;
    return;
  }
  container.innerHTML = projects
    .map(
      (p) => `
      <label class="checklist-item">
        <input type="checkbox" value="${escapeHtml(p.id)}" ${selected.has(p.id) ? "checked" : ""} />
        <span class="checklist-name">${escapeHtml(p.name)}</span>
        <span class="checklist-agent">${escapeHtml(p.agent)}</span>
      </label>`
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
  if (lastSelectedAgentId && sorted.some((a) => a.id === lastSelectedAgentId)) {
    select.value = lastSelectedAgentId;
  }
}

function resetAgentForm() {
  editingAgentId = null;
  (document.getElementById("agent-name") as HTMLInputElement).value = "";
  (document.getElementById("agent-command") as HTMLInputElement).value = "";
  (document.getElementById("agent-submit") as HTMLButtonElement).textContent = "Add agent";
  document.getElementById("agent-cancel")!.classList.add("hidden");
}

function resetGroupForm() {
  editingGroupId = null;
  (document.getElementById("group-name") as HTMLInputElement).value = "";
  (document.getElementById("group-submit") as HTMLButtonElement).textContent = "Add group";
  document.getElementById("group-cancel")!.classList.add("hidden");
  renderGroupChecklist(new Set());
}

async function refreshAgents() {
  const form = document.getElementById("agent-form");
  const list = document.getElementById("agents-list")!;
  const inList = !!form && !form.classList.contains("hidden") && form.parentElement === list;
  if (inList) form!.remove();
  agents = await invoke<AgentInfo[]>("list_agents");
  renderAgents();
  renderAgentOptions();
  renderAgentFilter();
  if (inList && form && editingAgentId) {
    const row = [...list.querySelectorAll(".agent-row")].find(
      (r) => r.getAttribute("data-id") === editingAgentId
    );
    if (row) row.insertAdjacentElement("afterend", form);
  }
}

async function refreshProjects() {
  const contextForm = document.getElementById("context-form");
  const projectsEl = document.getElementById("projects")!;
  const inList =
    !!contextForm &&
    !contextForm.classList.contains("hidden") &&
    contextForm.parentElement === projectsEl;
  if (inList) contextForm!.remove();
  projects = await invoke<Project[]>("list_projects");
  const statuses = await Promise.all(
    projects.map(async (project) => {
      try {
        return [project.id, await invoke<ContextStatus>("context_status", { projectId: project.id })] as const;
      } catch {
        return null;
      }
    })
  );
  contextStatuses = new Map(statuses.filter((item): item is readonly [string, ContextStatus] => item !== null));
  renderAgentFilter();
  renderProjects();
  if (inList && contextForm && editingContextProjectId) {
    const card = [...document.querySelectorAll("#projects .card")].find(
      (c) => c.getAttribute("data-id") === editingContextProjectId
    );
    if (card) card.insertAdjacentElement("afterend", contextForm);
  }
}

function showContextMode(mode: "edit" | "preview") {
  document.querySelectorAll<HTMLElement>("#context-form .tab").forEach((tab) =>
    tab.classList.toggle("active", tab.dataset.mode === mode)
  );
  const text = document.getElementById("context-text") as HTMLTextAreaElement;
  const preview = document.getElementById("context-preview")!;
  if (mode === "preview") {
    preview.innerHTML = marked.parse(text.value, { renderer: previewRenderer }) as string;
    text.classList.add("hidden");
    preview.classList.remove("hidden");
  } else {
    text.classList.remove("hidden");
    preview.classList.add("hidden");
  }
}

async function loadContextEditor(projectId: string) {
  const project = projects.find((item) => item.id === projectId);
  const snapshot = await invoke<ReadContextResult>("read_context", { projectId });
  editingContextProjectId = projectId;
  editingContextSnapshot = snapshot;
  contextDirty = false;
  (document.getElementById("context-text") as HTMLTextAreaElement).value = snapshot.content;
  document.getElementById("context-project-label")!.textContent =
    `${project?.name ?? "project"} · ${project?.agent ?? ""} -> ${CONTEXT_FILES}`;
  showContextMode("edit");
  const form = document.getElementById("context-form")!;
  form.classList.remove("hidden");
  const targetCard = [...document.querySelectorAll("#projects .card")].find(
    (c) => c.getAttribute("data-id") === projectId
  );
  if (targetCard) targetCard.insertAdjacentElement("afterend", form);
}

async function discardContextChanges(): Promise<boolean> {
  if (!contextDirty) return true;
  return confirm("Discard unsaved context changes?", {
    title: "Unsaved changes",
    kind: "warning",
  });
}

function closeContextEditor() {
  editingContextProjectId = null;
  editingContextSnapshot = null;
  contextDirty = false;
  const cf = document.getElementById("context-form");
  if (cf) {
    cf.classList.add("hidden");
    if (cf.parentElement === document.getElementById("projects")) {
      document.getElementById("projects")!.after(cf);
    }
  }
}

async function refreshGroups() {
  const form = document.getElementById("group-form");
  const list = document.getElementById("groups")!;
  const inList = !!form && !form.classList.contains("hidden") && form.parentElement === list;
  if (inList) form!.remove();
  groups = await invoke<Group[]>("list_groups");
  renderGroups();
  if (inList && form && editingGroupId) {
    const card = [...list.querySelectorAll(".card")].find(
      (c) => c.getAttribute("data-id") === editingGroupId
    );
    if (card) card.insertAdjacentElement("afterend", form);
  }
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
  applyTab(activeTab);
  document.getElementById("main-tabs")!.addEventListener("click", (e) => {
    const btn = (e.target as HTMLElement).closest(".tab-btn") as HTMLElement | null;
    if (!btn) return;
    const tab = btn.dataset.tab!;
    if (tab === activeTab) return;
    applyTab(tab);
    localStorage.setItem("termana.activeTab", tab);
  });

  await refreshAgents();
  await refreshProjects();
  await refreshGroups();

  document.getElementById("toggle-add-project")!.addEventListener("click", () => {
    document.getElementById("add-form")!.classList.toggle("hidden");
  });
  document.getElementById("toggle-add-group")!.addEventListener("click", () => {
    resetGroupForm();
    const form = document.getElementById("group-form")!;
    document.getElementById("groups")!.before(form);
    form.classList.toggle("hidden");
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
        await refreshProjects();
      } catch (err) {
        await message(String(err), { title: "Launch failed", kind: "error" });
      }
    };
    if (target.classList.contains("delete")) {
      const ok = await confirm("Remove this project?", { title: "Remove project", kind: "warning" });
      if (!ok) return;
      await invoke("remove_project", { id });
      await refreshProjects();
      await refreshGroups();
      return;
    }
    if (target.classList.contains("context")) {
      if (!(await discardContextChanges())) return;
      try {
        await loadContextEditor(id);
      } catch (err) {
        await message(String(err), { title: "Load failed", kind: "error" });
      }
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

  // project sort mode (left-to-right buttons; name/added toggle asc/desc)
  const sortBar = document.getElementById("project-sort")!;

  const updateSortButtons = () => {
    sortBar.querySelectorAll<HTMLElement>(".sort-btn").forEach((btn) => {
      const mode = btn.dataset.mode!;
      let active = false;
      let dir = "";
      if (mode === "manual") active = projectSortMode === "manual";
      else if (mode === "recent") active = projectSortMode === "recent";
      else if (mode === "name") {
        active = projectSortMode === "name-asc" || projectSortMode === "name-desc";
        dir = projectSortMode === "name-asc" ? " ↑" : " ↓";
      } else if (mode === "added") {
        active = projectSortMode === "added-asc" || projectSortMode === "added-desc";
        dir = projectSortMode === "added-asc" ? " ↑" : " ↓";
      }
      btn.classList.toggle("active", active);
      btn.textContent = (btn.dataset.label ?? mode) + dir;
    });
  };

  sortBar.addEventListener("click", (e) => {
    const btn = (e.target as HTMLElement).closest(".sort-btn") as HTMLElement | null;
    if (!btn) return;
    const mode = btn.dataset.mode!;
    if (mode === "name" || mode === "added") {
      if (mode === "name") {
        projectSortMode = projectSortMode === "name-asc" ? "name-desc" : "name-asc";
      } else {
        projectSortMode = projectSortMode === "added-asc" ? "added-desc" : "added-asc";
      }
    } else {
      projectSortMode = mode;
    }
    localStorage.setItem("termana.projectSort", projectSortMode);
    updateSortButtons();
    renderProjects();
  });

  updateSortButtons();

  // agent filter dropdown ("全部" + one entry per agent, with project counts)
  const filterBtn = document.getElementById("agent-filter-btn")!;
  const filterMenu = document.getElementById("agent-filter-menu")!;

  const closeFilterMenu = () => filterMenu.classList.add("hidden");

  filterBtn.addEventListener("click", (e) => {
    e.stopPropagation();
    filterMenu.classList.toggle("hidden");
  });

  filterMenu.addEventListener("click", (e) => {
    const item = (e.target as HTMLElement).closest(".filter-item") as HTMLElement | null;
    if (!item) return;
    projectAgentFilter = item.dataset.agent!;
    localStorage.setItem("termana.projectAgentFilter", projectAgentFilter);
    closeFilterMenu();
    renderAgentFilter();
    renderProjects();
  });

  document.addEventListener("click", (e) => {
    if (!(e.target as HTMLElement).closest("#agent-filter")) closeFilterMenu();
  });

  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") closeFilterMenu();
  });

  // project drag-to-reorder (manual order) — floating ghost + placeholder
  const projectsEl = document.getElementById("projects")!;
  let dragId: string | null = null;
  let dragOriginal: HTMLElement | null = null;
  let dragClone: HTMLElement | null = null;
  let dragPlaceholder: HTMLElement | null = null;
  let offsetX = 0;
  let offsetY = 0;

  projectsEl.addEventListener("pointerdown", (e) => {
    if (projectSortMode !== "manual" || projectAgentFilter !== "all") return;
    const handle = (e.target as HTMLElement).closest(".drag-handle") as HTMLElement | null;
    if (!handle) return;
    const card = handle.closest(".card") as HTMLElement | null;
    if (!card) return;
    e.preventDefault();
    dragId = card.getAttribute("data-id");
    dragOriginal = card;
    const rect = card.getBoundingClientRect();
    offsetX = e.clientX - rect.left;
    offsetY = e.clientY - rect.top;

    dragClone = card.cloneNode(true) as HTMLElement;
    dragClone.classList.add("drag-clone");
    dragClone.style.width = `${rect.width}px`;
    dragClone.style.height = `${rect.height}px`;
    dragClone.style.left = `${rect.left}px`;
    dragClone.style.top = `${rect.top}px`;
    document.body.appendChild(dragClone);

    dragPlaceholder = document.createElement("div");
    dragPlaceholder.className = "drag-placeholder";
    dragPlaceholder.style.height = `${rect.height}px`;
    projectsEl.insertBefore(dragPlaceholder, card);
    card.style.display = "none";
    handle.setPointerCapture(e.pointerId);
  });

  window.addEventListener("pointermove", (e) => {
    if (!dragId || !dragClone || !dragPlaceholder) return;
    dragClone.style.left = `${e.clientX - offsetX}px`;
    dragClone.style.top = `${e.clientY - offsetY}px`;
    const cards = [...projectsEl.querySelectorAll(".card")].filter(
      (c) => (c as HTMLElement).style.display !== "none"
    ) as HTMLElement[];
    let inserted = false;
    for (const c of cards) {
      const box = c.getBoundingClientRect();
      if (e.clientY < box.top + box.height / 2) {
        projectsEl.insertBefore(dragPlaceholder, c);
        inserted = true;
        break;
      }
    }
    if (!inserted) projectsEl.appendChild(dragPlaceholder);
  });

  const finishDrag = () => {
    if (!dragId) return;
    dragClone?.remove();
    if (dragPlaceholder && dragOriginal) {
      dragPlaceholder.replaceWith(dragOriginal);
      dragOriginal.style.display = "";
    } else {
      dragPlaceholder?.remove();
    }
    const orderedIds = [...projectsEl.querySelectorAll(".card")]
      .map((c) => c.getAttribute("data-id"))
      .filter((id): id is string => !!id);
    const byId = new Map(projects.map((pr) => [pr.id, pr]));
    projects = orderedIds.map((id) => byId.get(id)!).filter((pr): pr is Project => !!pr);
    dragId = null;
    dragOriginal = null;
    dragClone = null;
    dragPlaceholder = null;
    invoke("reorder_projects", { orderedIds }).catch(() => {});
    projectSortMode = "manual";
    localStorage.setItem("termana.projectSort", projectSortMode);
    updateSortButtons();
    renderProjects();
  };

  window.addEventListener("pointerup", finishDrag);
  window.addEventListener("pointercancel", finishDrag);

  document.getElementById("add-form")!.addEventListener("submit", async (e) => {
    e.preventDefault();
    if (!pathValid) return;
    const name = (document.getElementById("name") as HTMLInputElement).value;
    const path = (document.getElementById("path") as HTMLInputElement).value;
    const agent = (document.getElementById("agent") as HTMLSelectElement).value;
    try {
      await invoke("add_project", { name, path, agent });
      lastSelectedAgentId = agent;
      (document.getElementById("name") as HTMLInputElement).value = "";
      (document.getElementById("path") as HTMLInputElement).value = "";
      pathValid = false;
      nameTouched = false;
      (document.getElementById("add-project-btn") as HTMLButtonElement).disabled = true;
      await refreshProjects();
      await refreshGroups();
    } catch (err) {
      await message(String(err), { title: "Add failed", kind: "error" });
    }
  });

  (document.getElementById("agent") as HTMLSelectElement).addEventListener("change", (e) => {
    lastSelectedAgentId = (e.target as HTMLSelectElement).value;
  });

  // context editor: save / cancel
  document.getElementById("context-form")!.addEventListener("submit", async (e) => {
    e.preventDefault();
    if (!editingContextProjectId || !editingContextSnapshot) return;
    const projectId = editingContextProjectId;
    const snapshot = editingContextSnapshot;
    const context = (document.getElementById("context-text") as HTMLTextAreaElement).value;
    let convertClaude = false;
    if (snapshot.requiresClaudeConversion) {
      convertClaude = await confirm(
        "CLAUDE.md contains independent content. Link it to AGENTS.md after saving? Cancel keeps CLAUDE.md unchanged.",
        { title: "Link CLAUDE.md", kind: "warning" }
      );
    }
    try {
      const result = await invoke<SaveContextResult>("save_context", {
        projectId,
        content: context,
        expectedAgentsRevision: snapshot.agentsRevision,
        expectedClaudeRevision: snapshot.claudeRevision,
        expectedLegacyRevision: snapshot.legacyRevision,
        convertClaude,
        migrateLegacy: snapshot.hasLegacyContext,
      });
      const notices = ["AGENTS.md saved."];
      let warning = false;
      if (result.claudeAction === "created") notices.push("CLAUDE.md now links to AGENTS.md.");
      if (result.claudeAction === "converted") notices.push("CLAUDE.md was converted to an AGENTS.md pointer.");
      if (result.claudeAction === "independent-kept") {
        notices.push("CLAUDE.md remains independent.");
        warning = true;
      }
      if (result.claudeAction === "symlink-skip") {
        notices.push("CLAUDE.md is a symlink and was left unchanged.");
      }
      if (result.claudeError) {
        notices.push(`CLAUDE.md was not updated: ${result.claudeError}`);
        warning = true;
      }
      if (result.legacyMigrated) notices.push("Legacy termana context was migrated.");
      if (result.legacyError) {
        notices.push(`Legacy context was retained: ${result.legacyError}`);
        warning = true;
      }
      if (result.hasMergeBlock) {
        notices.push("An unresolved merge block remains in AGENTS.md.");
        warning = true;
      }
      await loadContextEditor(projectId);
      await refreshProjects();
      await message(notices.join("\n\n"), {
        title: warning ? "Saved with warnings" : "Saved",
        kind: warning ? "warning" : "info",
      });
    } catch (err) {
      await message(String(err), { title: "Save failed", kind: "error" });
    }
  });

  document.getElementById("context-text")!.addEventListener("input", () => {
    contextDirty = true;
  });

  document.getElementById("context-cancel")!.addEventListener("click", async () => {
    if (!(await discardContextChanges())) return;
    closeContextEditor();
  });

  // context editor: edit / preview tabs
  document.querySelectorAll<HTMLButtonElement>("#context-form .tab").forEach((tab) => {
    tab.addEventListener("click", () => {
      showContextMode(tab.dataset.mode === "preview" ? "preview" : "edit");
    });
  });

  // group card: ✕ = confirm-remove, ✎ = edit, Launch all ▸ = direct,
  // card body = confirm-then-launch-all (same as project cards)
  document.getElementById("groups")!.addEventListener("click", async (e) => {
    const target = e.target as HTMLElement;
    const card = target.closest(".card") as HTMLElement | null;
    if (!card) return;
    const id = card.getAttribute("data-id");
    if (!id) return;
    const launchAll = async () => {
      try {
        await invoke("launch_group", { id });
      } catch (err) {
        await message(String(err), { title: "Launch failed", kind: "error" });
      }
    };
    if (target.classList.contains("delete")) {
      const ok = await confirm("Remove this group?", { title: "Remove group", kind: "warning" });
      if (!ok) return;
      await invoke("remove_group", { id });
      await refreshGroups();
      return;
    }
    if (target.classList.contains("edit")) {
      const g = groups.find((x) => x.id === id);
      if (!g) return;
      editingGroupId = id;
      (document.getElementById("group-name") as HTMLInputElement).value = g.name;
      renderGroupChecklist(new Set(g.projectIds));
      (document.getElementById("group-submit") as HTMLButtonElement).textContent = "Update group";
      document.getElementById("group-cancel")!.classList.remove("hidden");
      const form = document.getElementById("group-form")!;
      form.classList.remove("hidden");
      const card = [...document.querySelectorAll("#groups .card")].find(
        (c) => c.getAttribute("data-id") === id
      );
      if (card) card.insertAdjacentElement("afterend", form);
      return;
    }
    if (target.classList.contains("card-launch")) {
      await launchAll();
      return;
    }
    const g = groups.find((x) => x.id === id);
    const ok = await confirm(`Launch all in “${g?.name ?? "group"}”?`, { title: "Launch group" });
    if (!ok) return;
    await launchAll();
  });

  document.getElementById("group-form")!.addEventListener("submit", async (e) => {
    e.preventDefault();
    const name = (document.getElementById("group-name") as HTMLInputElement).value;
    const projectIds = [
      ...document.querySelectorAll<HTMLInputElement>("#group-projects input:checked"),
    ].map((cb) => cb.value);
    try {
      if (editingGroupId) {
        await invoke("update_group", { id: editingGroupId, name, projectIds });
        resetGroupForm();
        const form = document.getElementById("group-form")!;
        form.classList.add("hidden");
        document.getElementById("groups")!.before(form);
      } else {
        await invoke("add_group", { name, projectIds });
        (e.target as HTMLFormElement).reset();
        renderGroupChecklist(new Set());
      }
      await refreshGroups();
    } catch (err) {
      await message(String(err), { title: "Save failed", kind: "error" });
    }
  });

  document.getElementById("group-cancel")!.addEventListener("click", () => {
    resetGroupForm();
    const form = document.getElementById("group-form")!;
    form.classList.add("hidden");
    document.getElementById("groups")!.before(form);
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
      const form = document.getElementById("agent-form")!;
      form.classList.remove("hidden");
      const row = [...document.querySelectorAll("#agents-list .agent-row")].find(
        (r) => r.getAttribute("data-id") === id
      );
      if (row) row.insertAdjacentElement("afterend", form);
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
        const form = document.getElementById("agent-form")!;
        form.classList.add("hidden");
        document.getElementById("agents-list")!.after(form);
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
    const form = document.getElementById("agent-form")!;
    form.classList.add("hidden");
    document.getElementById("agents-list")!.after(form);
  });

  // custom tooltip: replaces native title and pins to the top-left of the cursor
  const tooltip = document.createElement("div");
  tooltip.id = "tooltip";
  tooltip.classList.add("hidden");
  document.body.appendChild(tooltip);

  const positionTooltip = (e: MouseEvent) => {
    const pad = 10;
    const w = tooltip.offsetWidth;
    const h = tooltip.offsetHeight;
    let left = e.clientX - pad - w;
    let top = e.clientY - pad - h;
    if (left < pad) left = e.clientX + pad;
    if (top < pad) top = e.clientY + pad;
    left = Math.min(Math.max(left, pad), window.innerWidth - w - pad);
    top = Math.min(Math.max(top, pad), window.innerHeight - h - pad);
    tooltip.style.left = `${left}px`;
    tooltip.style.top = `${top}px`;
  };

  let tooltipOwner: HTMLElement | null = null;

  const hideTooltip = () => {
    if (tooltipOwner) {
      tooltipOwner.setAttribute("title", tooltipOwner.dataset.tooltip ?? "");
      delete tooltipOwner.dataset.tooltip;
      tooltipOwner = null;
    }
    tooltip.classList.add("hidden");
  };

  document.addEventListener("mouseover", (e) => {
    const el = (e.target as HTMLElement).closest("[title]") as HTMLElement | null;
    if (!el) return;
    const text = el.getAttribute("title");
    if (!text) return;
    if (tooltipOwner && tooltipOwner !== el) hideTooltip();
    el.dataset.tooltip = text;
    el.removeAttribute("title");
    tooltipOwner = el;
    tooltip.textContent = text;
    tooltip.classList.remove("hidden");
    positionTooltip(e as MouseEvent);
  });

  document.addEventListener("mousemove", (e) => {
    if (tooltip.classList.contains("hidden")) return;
    // the hovered element can vanish without ever firing mouseout: a drag hides it,
    // or a re-render replaces the node. Drop the tooltip once it is no longer rendered.
    if (!tooltipOwner?.isConnected || tooltipOwner.getClientRects().length === 0) {
      hideTooltip();
      return;
    }
    positionTooltip(e);
  });

  document.addEventListener("mouseout", (e) => {
    const el = (e.target as HTMLElement).closest("[data-tooltip]") as HTMLElement | null;
    if (!el) return;
    const related = e.relatedTarget as Node | null;
    if (related && el.contains(related)) return;
    hideTooltip();
  });

  // a press starts a drag/click that may hide or re-render the hovered element
  document.addEventListener("pointerdown", hideTooltip);
});
