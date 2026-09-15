import type {
  CreateWorkspaceRequest,
  ExecRequest,
  ExecResult,
  PromptRequest,
  PromptResult,
  WorkspaceMeta,
} from "./types";

const DEFAULT_BASE = "http://127.0.0.1:8787";

function baseUrl(): string {
  const raw = import.meta.env.VITE_API_BASE_URL as string | undefined;
  return (raw && raw.trim()) || DEFAULT_BASE;
}

/** HTTP error with the status attached so callers can branch on 404/405/501. */
export class ApiHttpError extends Error {
  readonly status: number;
  readonly method: string;
  readonly path: string;

  constructor(opts: { status: number; method: string; path: string; message: string }) {
    super(opts.message);
    this.name = "ApiHttpError";
    this.status = opts.status;
    this.method = opts.method;
    this.path = opts.path;
  }
}

async function request<T>(
  path: string,
  init?: RequestInit,
): Promise<T> {
  const method = (init?.method ?? "GET").toUpperCase();
  let res: Response;
  try {
    res = await fetch(`${baseUrl()}${path}`, {
      ...init,
      headers: {
        "Content-Type": "application/json",
        ...(init?.headers ?? {}),
      },
    });
  } catch (err) {
    const detail = err instanceof Error ? err.message : String(err);
    throw new Error(
      `Cannot reach tk-cloud at ${baseUrl()} (${method} ${path}). ${detail}`,
    );
  }
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new ApiHttpError({
      status: res.status,
      method,
      path,
      message: `API ${method} ${path} → ${res.status}${text ? `: ${text}` : ""}`,
    });
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

export async function getHealth(): Promise<{ ok?: boolean; status?: string } | unknown> {
  return request("/health");
}

export async function listWorkspaces(): Promise<WorkspaceMeta[]> {
  return request<WorkspaceMeta[]>("/v1/workspaces");
}

export async function createWorkspace(
  body?: CreateWorkspaceRequest,
): Promise<WorkspaceMeta> {
  return request<WorkspaceMeta>("/v1/workspaces", {
    method: "POST",
    body: JSON.stringify(body ?? {}),
  });
}

export async function getWorkspace(id: string): Promise<WorkspaceMeta> {
  return request<WorkspaceMeta>(`/v1/workspaces/${encodeURIComponent(id)}`);
}

export async function exec(
  id: string,
  req: ExecRequest,
): Promise<ExecResult> {
  return request<ExecResult>(`/v1/workspaces/${encodeURIComponent(id)}/exec`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

/**
 * Fallback when `POST /v1/workspaces/:id/prompt` is not implemented yet:
 * the server-side `tk exec` agent loop runs the prompt instead.
 * Documented in `apps/web/README.md`; the Worker is not implemented here.
 */
export function buildPromptExecFallback(prompt: string): ExecRequest {
  return { source: `tk exec ${quoteForShell(prompt)}` };
}

function quoteForShell(s: string): string {
  if (s === "") return "''";
  if (/^[A-Za-z0-9_@%+=:,./-]+$/.test(s)) return s;
  return `'${s.replace(/'/g, "'\\''")}'`;
}

/** True when the prompt endpoint itself is missing (not a prompt failure). */
export function isPromptEndpointMissing(err: unknown): boolean {
  return (
    err instanceof ApiHttpError &&
    (err.status === 404 || err.status === 405 || err.status === 501)
  );
}

/**
 * Send an agent prompt to a workspace.
 *
 * Tries `POST /v1/workspaces/:id/prompt` first; when the server does not
 * implement it yet (404/405/501), falls back to `POST .../exec` with a
 * `tk exec <prompt>` command string and marks the result `fallback: "exec"`.
 */
export async function promptWorkspace(
  id: string,
  req: PromptRequest,
): Promise<PromptResult> {
  const path = `/v1/workspaces/${encodeURIComponent(id)}/prompt`;
  try {
    return await request<PromptResult>(path, {
      method: "POST",
      body: JSON.stringify(req),
    });
  } catch (err) {
    if (!isPromptEndpointMissing(err)) throw err;
    const fallback = await exec(id, buildPromptExecFallback(req.prompt));
    return {
      ok: fallback.ok,
      text: fallback.stdout ?? fallback.message,
      stdout: fallback.stdout,
      stderr: fallback.stderr,
      message: fallback.message,
      exitCode: fallback.exitCode,
      backend: fallback.backend,
      stub: fallback.stub,
      fallback: "exec",
    };
  }
}

/**
 * Buffered agent text from a prompt result. Accepts `text`/`output`/
 * `stdout`/`message` because the prompt endpoint is still landing
 * server-side and shapes may vary.
 */
export function promptText(result: PromptResult): string | undefined {
  for (const field of [result.text, result.output, result.stdout, result.message]) {
    if (typeof field === "string" && field.trim()) return field;
  }
  return undefined;
}

export { baseUrl };
