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

export interface CreateWorkspaceRequest {
  name?: string;
  tier?: WorkspaceTier | string;
}
