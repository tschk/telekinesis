export type HostMode = "local" | "cloud";

export type WorkspaceStatus = "ready" | "starting" | "stopped" | "error";

export interface Workspace {
  id: string;
  name: string;
  status: WorkspaceStatus;
  region?: string;
  updatedAt?: string;
}

export interface SpawnResult {
  ok: boolean;
  binary?: string | null;
  message: string;
}
