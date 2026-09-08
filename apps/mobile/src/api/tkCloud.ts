/** tk-cloud HTTP client + protocol types (mirrored; no package dep). */

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

export interface CreateWorkspaceRequest {
  name?: string;
  tier?: WorkspaceTier | string;
}

export interface ListWorkspacesResult {
  workspaces: WorkspaceMeta[];
  /** Present when GET /v1/workspaces returned 404/501 (endpoint not ready). */
  note?: string;
}

const DEFAULT_BASE = "http://127.0.0.1:8787";

export function baseUrl(): string {
  const raw = process.env.EXPO_PUBLIC_TK_CLOUD_URL;
  return (raw && raw.trim()) || DEFAULT_BASE;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${baseUrl()}${path}`, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      ...(init?.headers ?? {}),
    },
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(
      `API ${init?.method ?? "GET"} ${path} → ${res.status}${text ? `: ${text}` : ""}`,
    );
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

export async function getHealth(): Promise<{ ok?: boolean; status?: string } | unknown> {
  return request("/health");
}

/**
 * List workspaces. HTTP 404/501 → empty list + note (list endpoint not ready).
 * Other non-OK statuses throw.
 */
export async function listWorkspaces(): Promise<ListWorkspacesResult> {
  const res = await fetch(`${baseUrl()}/v1/workspaces`, {
    headers: { "Content-Type": "application/json" },
  });
  if (res.status === 404 || res.status === 501) {
    return {
      workspaces: [],
      note: `GET /v1/workspaces returned ${res.status} — list endpoint unavailable; create/get by id may still work.`,
    };
  }
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(
      `API GET /v1/workspaces → ${res.status}${text ? `: ${text}` : ""}`,
    );
  }
  const data = (await res.json()) as WorkspaceMeta[];
  return { workspaces: Array.isArray(data) ? data : [] };
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
