export interface LogPaneProps {
  lines: string[];
  emptyHint?: string;
}

export function LogPane({
  lines,
  emptyHint = "Exec stdout/stderr will appear here.",
}: LogPaneProps) {
  return (
    <div className="tk-log" role="log" aria-live="polite">
      {lines.length === 0 ? (
        <div className="tk-log__empty">{emptyHint}</div>
      ) : (
        <pre className="tk-log__body">{lines.join("\n")}</pre>
      )}
    </div>
  );
}
