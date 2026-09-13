import { useState } from "react";
import { Button } from "./Button";

interface Props {
  disabled?: boolean;
  busy?: boolean;
  onRun: (source: string) => void | Promise<void>;
}

export function PromptBox({ disabled, busy, onRun }: Props) {
  const [source, setSource] = useState("");

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const trimmed = source.trim();
    if (!trimmed || disabled || busy) return;
    await onRun(trimmed);
  }

  return (
    <form className="tk-prompt" onSubmit={handleSubmit}>
      <textarea
        className="tk-textarea"
        rows={4}
        placeholder="Prompt / shell source to exec in the selected workspace…"
        value={source}
        disabled={disabled || busy}
        onChange={(e) => setSource(e.target.value)}
      />
      <div className="tk-prompt__footer">
        <Button type="submit" disabled={disabled || busy || !source.trim()}>
          {busy ? "Running…" : "Run"}
        </Button>
      </div>
    </form>
  );
}
