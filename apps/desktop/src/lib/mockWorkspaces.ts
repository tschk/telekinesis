import type { Workspace } from "./types";

/** In-app mock used when the cloud API is unreachable or unset. */
export const MOCK_WORKSPACES: Workspace[] = [
  {
    id: "ws_local_dev",
    name: "dev-sandbox",
    status: "ready",
    region: "local-mock",
    updatedAt: new Date().toISOString(),
  },
  {
    id: "ws_cloud_alpha",
    name: "alpha-workspace",
    status: "starting",
    region: "cf-do-us",
    updatedAt: new Date().toISOString(),
  },
  {
    id: "ws_cloud_beta",
    name: "beta-review",
    status: "stopped",
    region: "cf-do-eu",
    updatedAt: new Date().toISOString(),
  },
];
