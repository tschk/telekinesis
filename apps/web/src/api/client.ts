import type {
  CreateWorkspaceRequest,
  ExecRequest,
  ExecResult,
  WorkspaceMeta,
} from "./types";

const DEFAULT_BASE = "http://127.0.0.1:8787";

function baseUrl(): string {
  const raw = import.meta.env.VITE_API_BASE_URL as string | undefined;
  return (raw && raw.trim()) || DEFAULT_BASE;
}

async function request<T>(
  path: string,
  init?: RequestInit,
): Promise<T> {
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

export { baseUrl };
