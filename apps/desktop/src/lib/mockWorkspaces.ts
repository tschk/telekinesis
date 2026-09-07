import type { Workspace } from "./types";

/** In-app mock used when the cloud API is unreachable (network/HTTP failure). */
export const MOCK_WORKSPACES: Workspace[] = [
  {
    id: "ws_local_dev",
    name: "dev-sandbox",
    tier: "free",
    createdAt: new Date().toISOString(),
    computerBackend: "isolate-shell",
    status: "ready",
  },
  {
    id: "ws_cloud_alpha",
    name: "alpha-workspace",
    tier: "pro",
    createdAt: new Date().toISOString(),
    computerBackend: "isolate-shell",
    status: "starting",
  },
  {
    id: "ws_cloud_beta",
    name: "beta-review",
    tier: "team",
    createdAt: new Date().toISOString(),
    computerBackend: null,
    status: "stopped",
  },
];
