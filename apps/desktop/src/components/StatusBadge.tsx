import type { WorkspaceStatus } from "../lib/types";

const LABEL: Record<WorkspaceStatus, string> = {
  ready: "ready",
  starting: "starting",
  stopped: "stopped",
  error: "error",
};

export function StatusBadge({ status }: { status: WorkspaceStatus }) {
  return (
    <span className={`tk-badge tk-badge--${status}`} title={status}>
      {LABEL[status]}
    </span>
  );
}
