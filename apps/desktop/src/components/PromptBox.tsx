import type { FormEvent, KeyboardEvent } from "react";
import { Button } from "./Button";

export interface PromptBoxProps {
  value: string;
  onChange: (value: string) => void;
  onSubmit: () => void;
  disabled?: boolean;
  placeholder?: string;
}

export function PromptBox({
  value,
  onChange,
  onSubmit,
  disabled = false,
  placeholder = "Shell command to exec in workspace…",
}: PromptBoxProps) {
  function handleSubmit(e: FormEvent) {
    e.preventDefault();
    if (!disabled && value.trim()) onSubmit();
  }

  function onKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      if (!disabled && value.trim()) onSubmit();
    }
  }

  return (
    <form className="tk-prompt" onSubmit={handleSubmit}>
      <textarea
        className="tk-prompt__input"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={onKeyDown}
        placeholder={placeholder}
        disabled={disabled}
        rows={3}
        spellCheck={false}
      />
      <div className="tk-prompt__actions">
        <p className="tk-prompt__hint">
          Ctrl/⌘+Enter · POST /v1/workspaces/:id/exec
        </p>
        <Button type="submit" disabled={disabled || !value.trim()}>
          Exec
        </Button>
      </div>
    </form>
  );
}
