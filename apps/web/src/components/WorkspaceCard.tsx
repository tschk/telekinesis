import type { WorkspaceMeta } from "../api/types";
import { StatusBadge } from "./StatusBadge";

interface Props {
  workspace: WorkspaceMeta;
  selected: boolean;
  onSelect: (id: string) => void;
}

function statusTone(status: string): "ok" | "warn" | "danger" | "muted" {
  const s = status.toLowerCase();
  if (s.includes("ready") || s.includes("active") || s === "ok") return "ok";
  if (s.includes("pending") || s.includes("starting")) return "warn";
  if (s.includes("error") || s.includes("fail")) return "danger";
  return "muted";
}

export function WorkspaceCard({ workspace, selected, onSelect }: Props) {
  return (
    <button
      type="button"
      className={`tk-wcard ${selected ? "tk-wcard--selected" : ""}`}
      onClick={() => onSelect(workspace.id)}
    >
      <div className="tk-wcard__row">
        <strong className="tk-wcard__name">{workspace.name || workspace.id}</strong>
        <StatusBadge tone={statusTone(workspace.status)} label={workspace.status} />
      </div>
      <div className="tk-wcard__meta">
        <span>{workspace.tier}</span>
        <span className="tk-wcard__sep">·</span>
        <span>{workspace.computerBackend ?? "no backend"}</span>
      </div>
      <code className="tk-wcard__id">{workspace.id}</code>
    </button>
  );
}
