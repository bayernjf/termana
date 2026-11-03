import { marked } from "marked";

export function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c] as string
  );
}

// Sanitizing renderer shared by every remote/user-controlled markdown surface
// (context preview, announcements): raw HTML is escaped, links lose their
// href, images become text. Never render remote markdown with the default
// marked renderer — it injects HTML unsanitized into the DOM.
const previewRenderer = new marked.Renderer();
previewRenderer.html = ({ text }) => escapeHtml(text);
previewRenderer.link = ({ tokens }) => previewRenderer.parser.parseInline(tokens);
previewRenderer.image = ({ text }) => escapeHtml(`[image: ${text}]`);

export function renderSafeMarkdown(source: string): Promise<string> {
  return marked.parse(source, { renderer: previewRenderer, async: true }) as Promise<string>;
}
