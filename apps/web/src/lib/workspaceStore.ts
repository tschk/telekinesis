import {
  parseWorkspaceIds,
  serializeWorkspaceIds,
} from "../api/client";

const KEY = "tk.workspaceIds";

/**
 * Web Storage is main-thread only, so getItem/setItem stay here; the raw
 * blob crosses to the worker for parsing and serialization.
 */
export async function loadWorkspaceIds(): Promise<string[]> {
  let raw: string | null;
  try {
    raw = localStorage.getItem(KEY);
  } catch {
    return [];
  }
  if (!raw) return [];
  return parseWorkspaceIds(raw);
}

export async function saveWorkspaceIds(ids: string[]): Promise<void> {
  const unique = [...new Set(ids.filter(Boolean))];
  const raw = await serializeWorkspaceIds(unique);
  localStorage.setItem(KEY, raw);
}

export async function addWorkspaceId(id: string): Promise<string[]> {
  const next = [...new Set([...(await loadWorkspaceIds()), id])];
  await saveWorkspaceIds(next);
  return next;
}

export async function syncWorkspaceIds(ids: string[]): Promise<void> {
  await saveWorkspaceIds(ids);
}
