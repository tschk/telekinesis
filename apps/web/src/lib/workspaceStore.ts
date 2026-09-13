const KEY = "tk.workspaceIds";

export function loadWorkspaceIds(): string[] {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((x): x is string => typeof x === "string");
  } catch {
    return [];
  }
}

export function saveWorkspaceIds(ids: string[]): void {
  const unique = [...new Set(ids.filter(Boolean))];
  localStorage.setItem(KEY, JSON.stringify(unique));
}

export function addWorkspaceId(id: string): string[] {
  const next = [...new Set([...loadWorkspaceIds(), id])];
  saveWorkspaceIds(next);
  return next;
}

export function syncWorkspaceIds(ids: string[]): void {
  saveWorkspaceIds(ids);
}
