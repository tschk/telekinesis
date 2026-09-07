import { MOCK_WORKSPACES } from "./mockWorkspaces";
import type { Workspace } from "./types";

const DEFAULT_API = "http://127.0.0.1:8787";

export function cloudApiBase(): string {
  const fromEnv = import.meta.env.VITE_TK_CLOUD_API as string | undefined;
  return (fromEnv && fromEnv.trim()) || DEFAULT_API;
}

/**
 * List cloud workspaces from tk-cloud API stub.
 * Falls back to in-app mock JSON when fetch fails (offline / no local stub).
 */
export async function listWorkspaces(): Promise<{
  workspaces: Workspace[];
  source: "api" | "mock";
  error?: string;
}> {
  const base = cloudApiBase().replace(/\/$/, "");
  const url = `${base}/v1/workspaces`;

  try {
    const res = await fetch(url, {
      headers: { Accept: "application/json" },
      signal: AbortSignal.timeout(2500),
    });
    if (!res.ok) {
      throw new Error(`HTTP ${res.status}`);
    }
    const data = (await res.json()) as { workspaces?: Workspace[] } | Workspace[];
    const workspaces = Array.isArray(data)
      ? data
      : Array.isArray(data.workspaces)
        ? data.workspaces
        : [];
    if (workspaces.length === 0) {
      return { workspaces: MOCK_WORKSPACES, source: "mock", error: "empty API response" };
    }
    return { workspaces, source: "api" };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      workspaces: MOCK_WORKSPACES,
      source: "mock",
      error: `Using mock data (${message}). API: ${url}`,
    };
  }
}
