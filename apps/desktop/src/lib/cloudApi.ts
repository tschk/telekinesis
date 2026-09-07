import { MOCK_WORKSPACES } from "./mockWorkspaces";
import type {
  ExecRequest,
  ExecResult,
  HealthResponse,
  Workspace,
  WorkspaceMeta,
} from "./types";

const DEFAULT_API = "http://127.0.0.1:8787";

export function cloudApiBase(): string {
  const fromEnv = import.meta.env.VITE_TK_CLOUD_API as string | undefined;
  return (fromEnv && fromEnv.trim()) || DEFAULT_API;
}

function baseUrl(): string {
  return cloudApiBase().replace(/\/$/, "");
}

function asWorkspace(meta: WorkspaceMeta): Workspace {
  // 1:1 map — UI model is WorkspaceMeta
  return {
    id: meta.id,
    name: meta.name,
    tier: meta.tier,
    createdAt: meta.createdAt,
    computerBackend: meta.computerBackend ?? null,
    status: meta.status,
  };
}

async function parseJson<T>(res: Response): Promise<T> {
  return (await res.json()) as T;
}

/**
 * GET /health — tk-cloud liveness / computer capability chip.
 */
export async function getHealth(): Promise<{
  health: HealthResponse | null;
  ok: boolean;
  error?: string;
}> {
  const url = `${baseUrl()}/health`;
  try {
    const res = await fetch(url, {
      headers: { Accept: "application/json" },
      signal: AbortSignal.timeout(2500),
    });
    if (!res.ok) {
      throw new Error(`HTTP ${res.status}`);
    }
    const health = await parseJson<HealthResponse>(res);
    return { health, ok: Boolean(health?.ok) };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { health: null, ok: false, error: message };
  }
}

/**
 * List cloud workspaces from tk-cloud M1 API.
 * Empty array from a successful response is valid (source: "api").
 * Mock fallback only on network / HTTP failure.
 */
export async function listWorkspaces(): Promise<{
  workspaces: Workspace[];
  source: "api" | "mock";
  error?: string;
}> {
  const url = `${baseUrl()}/v1/workspaces`;

  try {
    const res = await fetch(url, {
      headers: { Accept: "application/json" },
      signal: AbortSignal.timeout(2500),
    });
    if (!res.ok) {
      throw new Error(`HTTP ${res.status}`);
    }
    const data = (await res.json()) as WorkspaceMeta[] | { workspaces?: WorkspaceMeta[] };
    const raw = Array.isArray(data)
      ? data
      : Array.isArray(data.workspaces)
        ? data.workspaces
        : [];
    return { workspaces: raw.map(asWorkspace), source: "api" };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      workspaces: MOCK_WORKSPACES,
      source: "mock",
      error: `Using mock data (${message}). API: ${url}`,
    };
  }
}

/** POST /v1/workspaces — create workspace (201 WorkspaceMeta). */
export async function createWorkspace(input?: {
  name?: string;
  tier?: string;
}): Promise<{ workspace: Workspace; error?: string }> {
  const url = `${baseUrl()}/v1/workspaces`;
  try {
    const res = await fetch(url, {
      method: "POST",
      headers: {
        Accept: "application/json",
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        name: input?.name,
        tier: input?.tier,
      }),
      signal: AbortSignal.timeout(8000),
    });
    if (!res.ok) {
      throw new Error(`HTTP ${res.status}`);
    }
    const meta = await parseJson<WorkspaceMeta>(res);
    return { workspace: asWorkspace(meta) };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      workspace: {
        id: "",
        name: input?.name ?? "workspace",
        tier: input?.tier ?? "free",
        createdAt: new Date().toISOString(),
        computerBackend: null,
        status: "error",
      },
      error: message,
    };
  }
}

/** GET /v1/workspaces/:id */
export async function getWorkspace(
  id: string,
): Promise<{ workspace: Workspace | null; error?: string }> {
  const url = `${baseUrl()}/v1/workspaces/${encodeURIComponent(id)}`;
  try {
    const res = await fetch(url, {
      headers: { Accept: "application/json" },
      signal: AbortSignal.timeout(2500),
    });
    if (!res.ok) {
      throw new Error(`HTTP ${res.status}`);
    }
    const meta = await parseJson<WorkspaceMeta>(res);
    return { workspace: asWorkspace(meta) };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { workspace: null, error: message };
  }
}

/** POST /v1/workspaces/:id/exec */
export async function execInWorkspace(
  id: string,
  body: ExecRequest,
): Promise<{ result: ExecResult; error?: string }> {
  const url = `${baseUrl()}/v1/workspaces/${encodeURIComponent(id)}/exec`;
  try {
    const res = await fetch(url, {
      method: "POST",
      headers: {
        Accept: "application/json",
        "Content-Type": "application/json",
      },
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(30000),
    });
    const result = await parseJson<ExecResult>(res);
    if (!res.ok && result.ok !== false) {
      return {
        result: {
          ok: false,
          message: result.message ?? `HTTP ${res.status}`,
          stdout: result.stdout,
          stderr: result.stderr,
          exitCode: result.exitCode,
          stub: result.stub,
          backend: result.backend,
        },
        error: `HTTP ${res.status}`,
      };
    }
    return { result };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      result: { ok: false, message },
      error: message,
    };
  }
}
