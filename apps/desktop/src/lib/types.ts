export type HostMode = "local" | "cloud";

/** Matches tk-cloud WorkspaceMeta (PR #1 / sha 802f4b2). */
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

/** UI model is 1:1 with WorkspaceMeta. */
export type Workspace = WorkspaceMeta;

export interface ExecRequest {
  source: string;
  backend?: ComputerBackend;
  cwd?: string;
}

export interface ExecResult {
  ok: boolean;
  stdout?: string;
  stderr?: string;
  exitCode?: number;
  stub?: boolean;
  message?: string;
  backend?: ComputerBackend | string;
}

export interface HealthResponse {
  ok: boolean;
  service?: string;
  env?: string;
  computer?: string;
}

export interface SpawnResult {
  ok: boolean;
  binary?: string | null;
  message: string;
}
