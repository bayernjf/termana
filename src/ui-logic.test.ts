import test from "node:test";
import assert from "node:assert/strict";
import {
  contextBadges,
  countByAgent,
  escapeHtml,
  filterByAgent,
  labelForAgent,
  orderProjects,
  projectNameFromPath,
  type ProjectLike,
} from "./ui-logic";

function project(id: string, name: string, agent: string, createdAt: number, lastLaunched?: number): ProjectLike {
  return { id, name, agent, createdAt, lastLaunched };
}

test("escapeHtml escapes every HTML metacharacter exactly once", () => {
  assert.equal(escapeHtml(`<script>alert("x & 'y'")</script>`), "&lt;script&gt;alert(&quot;x &amp; &#39;y&#39;&quot;)&lt;/script&gt;");
  assert.equal(escapeHtml("plain text"), "plain text");
  assert.equal(escapeHtml("&amp;"), "&amp;amp;");
});

test("orderProjects supports every persisted sort mode", () => {
  const list = [
    project("a", "beta", "codex", 30, 100),
    project("b", "Alpha", "claude", 10, undefined),
    project("c", "gamma", "codex", 20, 200),
  ];

  assert.deepEqual(orderProjects(list, "name-asc").map((p) => p.id), ["b", "a", "c"]);
  assert.deepEqual(orderProjects(list, "name-desc").map((p) => p.id), ["c", "a", "b"]);
  assert.deepEqual(orderProjects(list, "added-asc").map((p) => p.id), ["b", "c", "a"]);
  assert.deepEqual(orderProjects(list, "added-desc").map((p) => p.id), ["a", "c", "b"]);
  assert.deepEqual(orderProjects(list, "recent").map((p) => p.id), ["c", "a", "b"]);
});

test("manual mode returns the original list and other modes never mutate it", () => {
  const list = [project("a", "beta", "codex", 30), project("b", "Alpha", "claude", 10)];
  const manual = orderProjects(list, "manual");
  assert.equal(manual, list, "manual mode must not copy or reorder");

  const sorted = orderProjects(list, "name-asc");
  assert.notEqual(sorted, list);
  assert.deepEqual(list.map((p) => p.id), ["a", "b"], "input order must be untouched");
});

test("unknown sort mode preserves order on a copy", () => {
  const list = [project("a", "beta", "codex", 30), project("b", "Alpha", "claude", 10)];
  const result = orderProjects(list, "future-mode");
  assert.notEqual(result, list);
  assert.deepEqual(result.map((p) => p.id), ["a", "b"]);
});

test("filterByAgent returns the same list for all and filters exactly otherwise", () => {
  const list = [project("a", "A", "codex", 1), project("b", "B", "claude", 2), project("c", "C", "codex", 3)];
  assert.equal(filterByAgent(list, "all"), list);
  assert.deepEqual(filterByAgent(list, "codex").map((p) => p.id), ["a", "c"]);
  assert.deepEqual(filterByAgent(list, "missing"), []);
});

test("labelForAgent prefers the configured name and falls back to the id", () => {
  const agents = [{ id: "codex", name: "Codex" }, { id: "claude", name: "Claude Code" }];
  assert.equal(labelForAgent(agents, "codex"), "Codex");
  assert.equal(labelForAgent(agents, "unknown"), "unknown");
});

test("countByAgent sorts by count then label and keeps a filtered agent at zero", () => {
  const projects = [
    project("a", "A", "codex", 1),
    project("b", "B", "codex", 2),
    project("c", "C", "claude", 3),
  ];
  const labels: Record<string, string> = { claude: "Claude Code", codex: "Codex", zeta: "Zeta" };
  const labelFor = (id: string) => labels[id] ?? id;

  assert.deepEqual(countByAgent(projects, "all", labelFor), [["codex", 2], ["claude", 1]]);
  assert.deepEqual(countByAgent(projects, "zeta", labelFor), [["codex", 2], ["claude", 1], ["zeta", 0]]);
  assert.deepEqual(countByAgent(projects, "claude", labelFor), [["codex", 2], ["claude", 1]]);
  assert.deepEqual(projects.map((p) => p.id), ["a", "b", "c"], "counting must not reorder the input");
});

test("projectNameFromPath handles unix, windows, trailing separators and root", () => {
  assert.equal(projectNameFromPath("/Users/me/termana"), "termana");
  assert.equal(projectNameFromPath("C:\\Users\\me\\termana"), "termana");
  assert.equal(projectNameFromPath("/Users/me/termana/"), "termana");
  assert.equal(projectNameFromPath("C:\\Users\\me\\termana\\"), "termana");
  assert.equal(projectNameFromPath("/"), "");
  assert.equal(projectNameFromPath(""), "");
});

test("contextBadges reports every warning in order and suppresses ctx", () => {
  assert.deepEqual(contextBadges({ divergent: true, hasMergeBlock: true, hasLegacyContext: true, agentsHasContent: true }), [
    { label: "divergent", tone: "warning", title: "AGENTS.md and CLAUDE.md differ" },
    { label: "merge", tone: "warning", title: "AGENTS.md has an unresolved merge block" },
    { label: "migrate", tone: "migration", title: "Legacy termana context will be migrated on save" },
  ]);
  assert.deepEqual(contextBadges({ divergent: false, hasMergeBlock: false, hasLegacyContext: false, agentsHasContent: true }), [
    { label: "ctx", tone: null, title: "AGENTS.md has context" },
  ]);
  assert.deepEqual(contextBadges({ divergent: false, hasMergeBlock: false, hasLegacyContext: false, agentsHasContent: false }), []);
});
