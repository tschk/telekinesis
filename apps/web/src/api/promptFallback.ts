import type { ExecRequest } from "./types";

/**
 * Fallback when `POST /v1/workspaces/:id/prompt` is not implemented yet:
 * the server-side `tk exec` agent loop runs the prompt instead.
 *
 * Kept free of network and JSON code so the worker can import it without
 * dragging in the page-side client.
 */
export function buildPromptExecFallback(prompt: string): ExecRequest {
  return { source: `tk exec ${quoteForShell(prompt)}` };
}

/** Shell-quote a prompt so `tk exec` receives it as a single argument. */
export function quoteForShell(s: string): string {
  if (s === "") return "''";
  if (/^[A-Za-z0-9_@%+=:,./-]+$/.test(s)) return s;
  return `'${s.replace(/'/g, "'\\''")}'`;
}
