import type { Workspace } from "../lib/types";
import { StatusBadge } from "./StatusBadge";

export interface WorkspaceCardProps {
  workspace: Workspace;
  selected?: boolean;
  onSelect?: (workspace: Workspace) => void;
}

export function WorkspaceCard({
  workspace,
  selected = false,
  onSelect,
}: WorkspaceCardProps) {
  return (
    <button
      type="button"
      className={`tk-workspace-card${selected ? " is-selected" : ""}`}
      onClick={() => onSelect?.(workspace)}
    >
      <div className="tk-workspace-card__row">
        <span className="tk-workspace-card__name">{workspace.name}</span>
        <StatusBadge status={workspace.status} />
      </div>
      <div className="tk-workspace-card__meta">
        <code>{workspace.id}</code>
        <span className="tk-workspace-card__chip">tier:{workspace.tier}</span>
        <span className="tk-workspace-card__chip">
          backend:{workspace.computerBackend ?? "—"}
        </span>
      </div>
    </button>
  );
}
