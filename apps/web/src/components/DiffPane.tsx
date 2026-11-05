import { parseDiff, type DiffLine } from "@tk/diff-view";
import { useRef } from "react";

interface Props {
  diff: string | null;
}

export function DiffPane({ diff }: Props) {
  const logRef = useRef<HTMLDivElement>(null);

  if (!diff) {
    return (
      <div className="tk-diff-stub">
        <p>No diff in the session yet.</p>
      </div>
    );
  }
  const lines = parseDiff(diff);
  const hasGitHeaders = lines.some((line) => line.text.startsWith("diff --git "));
  const files = lines.flatMap((line, index) =>
    line.kind === "meta" && (hasGitHeaders ? line.text.startsWith("diff --git ") : line.text.startsWith("--- "))
      ? [{ index, label: line.text }]
      : [],
  );
  const hunks = lines.flatMap((line, index) => line.kind === "hunk" ? [{ index, label: line.text }] : []);
  const additions = lines.filter((line) => line.kind === "add").length;
  const deletions = lines.filter((line) => line.kind === "del").length;

  function jumpTo(index: number) {
    logRef.current?.querySelector(`[data-diff-line="${index}"]`)?.scrollIntoView({ block: "nearest" });
  }

  return (
    <div className="tk-diff" role="region" aria-label="Workspace diff">
      <p className="tk-diff__summary">
        {files.length} {files.length === 1 ? "file" : "files"} · {hunks.length} {hunks.length === 1 ? "hunk" : "hunks"} · +{additions} / −{deletions}
      </p>
      {(files.length > 0 || hunks.length > 0) && (
        <nav className="tk-diff__nav" aria-label="Diff navigation">
          {[...files, ...hunks].sort((a, b) => a.index - b.index).map((entry) => (
            <button type="button" key={entry.index} onClick={() => jumpTo(entry.index)}>{entry.label}</button>
          ))}
        </nav>
      )}
      <div className="tk-log tk-diff__log" ref={logRef} tabIndex={0} aria-label="Diff contents">
        {lines.map((line, index) => (
          <pre key={`${index}-${line.text}`} data-diff-line={index} className={diffClass(line)}>
            {line.text || " "}
          </pre>
        ))}
      </div>
    </div>
  );
}

function diffClass(line: DiffLine): string {
  if (line.kind === "add") return "tk-log__line tk-log__line--ok";
  if (line.kind === "del") return "tk-log__line tk-log__line--danger";
  if (line.kind === "hunk" || line.kind === "meta") return "tk-log__line tk-log__line--warn";
  return "tk-log__line tk-log__line--muted";
}
