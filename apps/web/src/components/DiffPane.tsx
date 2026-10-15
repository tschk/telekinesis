import { parseDiff, type DiffLine } from "@tk/diff-view";

interface Props {
  diff: string | null;
}

export function DiffPane({ diff }: Props) {
  if (!diff) {
    return (
      <div className="tk-diff-stub">
        <p>No diff in the session yet.</p>
      </div>
    );
  }
  return (
    <div className="tk-log" role="region" aria-label="Workspace diff">
      {parseDiff(diff).map((line, index) => (
        <pre key={`${index}-${line.text}`} className={diffClass(line)}>
          {line.text || " "}
        </pre>
      ))}
    </div>
  );
}

function diffClass(line: DiffLine): string {
  if (line.kind === "add") return "tk-log__line tk-log__line--ok";
  if (line.kind === "del") return "tk-log__line tk-log__line--danger";
  if (line.kind === "hunk" || line.kind === "meta") return "tk-log__line tk-log__line--warn";
  return "tk-log__line tk-log__line--muted";
}
