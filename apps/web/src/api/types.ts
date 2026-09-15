/** Shared API types mirrored from tk-cloud protocol (no hard package dep). */

export type WorkspaceTier = "free" | "pro" | "team";

export type ComputerBackend = "isolate-shell" | "container" | "isolate-js";

export interface WorkspaceMeta {
  id: string;
  name: string;
  tier: WorkspaceTier | string;
  createdAt: string;
  computerBackend: ComputerBackend | string | null;
  status: string;
}

export interface ExecRequest {
  source: string;
  backend?: ComputerBackend;
  cwd?: string;
}

export interface ExecResult {
  ok: boolean;
  stdout?: string;
  stderr?: string;
  stub?: boolean;
  message?: string;
  exitCode?: number;
  backend?: ComputerBackend | string;
}

/**
 * Agent prompt turn in a workspace.
 *
 * Mirrors the in-progress tk-cloud `POST /v1/workspaces/:id/prompt`
 * endpoint (another agent is adding it server-side). Prompts go to
 * tk-cloud; model keys stay on the server/workspace, never in this app.
 */
export interface PromptRequest {
  prompt: string;
  model?: string;
  cwd?: string;
}

export interface PromptResult {
  ok: boolean;
  /** Agent text (buffered; streaming/SSE is a future enhancement). */
  text?: string;
  /** Accepted aliases — some servers return `output` or `stdout`. */
  output?: string;
  stdout?: string;
  stderr?: string;
  message?: string;
  exitCode?: number;
  backend?: ComputerBackend | string;
  stub?: boolean;
  /**
   * Present when the client fell back to `POST .../exec` because the
   * server does not implement `POST .../prompt` yet (404/405/501).
   */
  fallback?: "exec";
}

export interface CreateWorkspaceRequest {
  name?: string;
  tier?: WorkspaceTier | string;
}
