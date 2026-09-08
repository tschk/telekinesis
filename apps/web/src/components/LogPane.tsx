import { useEffect, useRef } from "react";

export interface LogLine {
  id: string;
  ts: string;
  text: string;
  tone?: "ok" | "danger" | "muted" | "warn";
}

interface Props {
  lines: LogLine[];
}

export function LogPane({ lines }: Props) {
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [lines]);

  return (
    <div className="tk-log" role="log" aria-live="polite">
      {lines.length === 0 ? (
        <p className="tk-log__empty">No exec output yet.</p>
      ) : (
        lines.map((line) => (
          <div key={line.id} className={`tk-log__line tk-log__line--${line.tone ?? "muted"}`}>
            <time className="tk-log__ts">{line.ts}</time>
            <pre className="tk-log__text">{line.text}</pre>
          </div>
        ))
      )}
      <div ref={endRef} />
    </div>
  );
}
