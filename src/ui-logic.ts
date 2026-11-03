export interface ProjectLike {
  id: string;
  name: string;
  agent: string;
  createdAt: number;
  lastLaunched?: number | null;
}

export interface AgentLike {
  id: string;
  name: string;
}

export interface ContextStatusLike {
  divergent: boolean;
  hasMergeBlock: boolean;
  hasLegacyContext: boolean;
  agentsHasContent: boolean;
}

export interface ContextBadge {
  label: string;
  tone: "warning" | "migration" | null;
  title: string;
}

const HTML_ESCAPES: Record<string, string> = {
  "&": "&amp;",
  "<": "&lt;",
  ">": "&gt;",
  '"': "&quot;",
  "'": "&#39;",
};

export function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/g, (char) => HTML_ESCAPES[char]);
}

export function orderProjects<T extends ProjectLike>(list: T[], mode: string): T[] {
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

export function filterByAgent<T extends ProjectLike>(list: T[], agentFilter: string): T[] {
  if (agentFilter === "all") return list;
  return list.filter((project) => project.agent === agentFilter);
}

export function labelForAgent(agents: AgentLike[], agentId: string): string {
  return agents.find((agent) => agent.id === agentId)?.name ?? agentId;
}

// Counts per agent, most projects first; a filtered-on agent stays at zero.
export function countByAgent(
  projects: ProjectLike[],
  agentFilter: string,
  labelFor: (agentId: string) => string,
): [string, number][] {
  const counts = new Map<string, number>();
  for (const project of projects) {
    counts.set(project.agent, (counts.get(project.agent) ?? 0) + 1);
  }
  if (agentFilter !== "all" && !counts.has(agentFilter)) {
    counts.set(agentFilter, 0);
  }
  return [...counts.entries()].sort(
    (a, b) => b[1] - a[1] || labelFor(a[0]).localeCompare(labelFor(b[0])),
  );
}

export function projectNameFromPath(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? "";
}

export function contextBadges(status: ContextStatusLike): ContextBadge[] {
  const badges: ContextBadge[] = [];
  if (status.divergent) {
    badges.push({
      label: "divergent",
      tone: "warning",
      title: "AGENTS.md and CLAUDE.md differ",
    });
  }
  if (status.hasMergeBlock) {
    badges.push({
      label: "merge",
      tone: "warning",
      title: "AGENTS.md has an unresolved merge block",
    });
  }
  if (status.hasLegacyContext) {
    badges.push({
      label: "migrate",
      tone: "migration",
      title: "Legacy termana context will be migrated on save",
    });
  }
  if (status.agentsHasContent && badges.length === 0) {
    badges.push({ label: "ctx", tone: null, title: "AGENTS.md has context" });
  }
  return badges;
}
