import { useState, type ChangeEvent } from "react";
import { Button } from "./Button";
import {
  MAX_CONTEXT_CHARACTERS,
  MAX_SOURCE_BYTES,
  parseChatExport,
  prepareContext,
  type ImportPreview,
  type PreparedContext,
} from "../lib/contextImport";
import { errorMessage } from "../lib/localApi";

export function ContextImport({
  onChange,
  locked,
}: {
  onChange: (context: PreparedContext | null) => void;
  locked: boolean;
}) {
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [budget, setBudget] = useState(12000);
  const [error, setError] = useState<string | null>(null);
  const [attached, setAttached] = useState(false);
  const prepared = preview
    ? prepareContext(preview, selected, { maxCharacters: budget })
    : null;
  async function readFile(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file) return;
    setError(null);
    setAttached(false);
    onChange(null);
    setSelected([]);
    setPreview(null);
    try {
      if (file.size > MAX_SOURCE_BYTES)
        throw new Error("Choose a chat export smaller than 10 MiB.");
      setPreview(parseChatExport(await file.text(), { fileName: file.name }));
    } catch (failure) {
      setError(errorMessage(failure));
    }
  }
  function clearAttachment() {
    setAttached(false);
    onChange(null);
  }
  return (
    <section className="context-import" aria-label="Imported reference context">
      <div className="context-heading">
        <span className="eyebrow">Reference context</span>
        {preview && (
          <Button
            variant="ghost"
            disabled={locked}
            onClick={() => {
              setPreview(null);
              setSelected([]);
              clearAttachment();
            }}
          >
            Remove
          </Button>
        )}
      </div>
      <p className="muted">
        Choose messages from a Claude JSON or Codex JSONL export. Only reviewed
        excerpts are attached to your next message.
      </p>
      <label className="file-control">
        Choose chat export
        <input
          type="file"
          accept=".json,.jsonl,application/json"
          disabled={locked}
          onChange={(e) => void readFile(e)}
        />
      </label>
      {error && (
        <p className="notice error" role="alert">
          {error}
        </p>
      )}
      {preview && (
        <>
          <p className="source-label">
            {preview.source.provider} · {preview.source.fileName}
          </p>
          {preview.warnings.map((warning) => (
            <p key={warning} className="muted">
              {warning}
            </p>
          ))}
          <div className="import-entries">
            {preview.conversations.map((conversation) => (
              <details
                key={conversation.id}
                open={preview.conversations.length === 1}
              >
                <summary>
                  {conversation.title}{" "}
                  <span className="muted">
                    {conversation.entries.length} messages
                  </span>
                </summary>
                {conversation.sourceUrl && (
                  <p className="source-label">
                    Source: {conversation.sourceUrl}
                  </p>
                )}
                {conversation.entries.map((entry) => (
                  <label key={entry.id} className="import-entry">
                    <input
                      type="checkbox"
                      disabled={locked}
                      checked={selected.includes(entry.id)}
                      onChange={(e) => {
                        setSelected((previous) =>
                          e.target.checked
                            ? [...previous, entry.id]
                            : previous.filter((id) => id !== entry.id),
                        );
                        clearAttachment();
                      }}
                    />
                    <span>
                      <strong>{entry.role}</strong>
                      <span className="entry-text">
                        {entry.text.slice(0, 800)}
                        {entry.text.length > 800 ? "…" : ""}
                      </span>
                      <details>
                        <summary>Read full message</summary>
                        <pre>{entry.text}</pre>
                      </details>
                    </span>
                  </label>
                ))}
              </details>
            ))}
          </div>
          <label className="budget-label">
            Character budget
            <input
              type="number"
              min={1000}
              max={MAX_CONTEXT_CHARACTERS}
              step={1000}
              value={budget}
              disabled={locked}
              onChange={(e) => {
                setBudget(
                  Math.max(
                    1000,
                    Math.min(
                      MAX_CONTEXT_CHARACTERS,
                      Number(e.target.value) || 1000,
                    ),
                  ),
                );
                clearAttachment();
              }}
            />
          </label>
          <p className="muted">
            {selected.length} selected · {prepared?.characters.toLocaleString()}{" "}
            / {budget.toLocaleString()} characters · ≈
            {prepared?.estimatedTokens.toLocaleString()} tokens (estimate)
          </p>
          {prepared?.truncated && (
            <p className="notice">
              Excerpts exceed this budget and will be shortened. Read the
              prepared text below.
            </p>
          )}
          {prepared?.text && (
            <details>
              <summary>Review prepared context</summary>
              <pre className="prepared-context">{prepared.text}</pre>
            </details>
          )}
          <Button
            disabled={locked || !prepared?.includedEntryIds.length}
            onClick={() => {
              onChange(prepared);
              setAttached(true);
            }}
          >
            {attached ? "Attached to next message" : "Attach selected excerpts"}
          </Button>
          {attached && (
            <p className="muted" role="status">
              Reference data, not instructions. Sending saves these excerpts in
              the thread.
            </p>
          )}
        </>
      )}
    </section>
  );
}
