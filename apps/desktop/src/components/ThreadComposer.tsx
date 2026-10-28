import { useRef, useState, type FormEvent, type KeyboardEvent } from "react";

export interface ThreadComposerProps {
  value: string;
  onChange: (value: string) => void;
  onSend: (text: string, mode: "steer" | "queue") => Promise<void>;
  onStop: () => Promise<void>;
  running: boolean;
  model?: string;
  onSettings?: () => void;
  onContext?: () => void;
  attachment?: string;
}

export function ThreadComposer({
  value,
  onChange,
  onSend,
  onStop,
  running,
  model,
  onSettings,
  onContext,
  attachment,
}: ThreadComposerProps) {
  const [sending, setSending] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [mode, setMode] = useState<"steer" | "queue">("steer");
  const sendingRef = useRef(false);
  const valueRef = useRef(value);
  valueRef.current = value;
  const input = useRef<HTMLTextAreaElement>(null);
  async function submit(event?: FormEvent) {
    event?.preventDefault();
    if (!value.trim() || sendingRef.current) return;
    const sent = value;
    sendingRef.current = true;
    setSending(true);
    setError(null);
    try {
      await onSend(sent.trim(), mode);
      if (valueRef.current === sent) onChange("");
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      sendingRef.current = false;
      setSending(false);
      input.current?.focus();
    }
  }
  async function stop() {
    if (stopping) return;
    setStopping(true);
    setError(null);
    try {
      await onStop();
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setStopping(false);
    }
  }
  function keyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (
      event.key === "Enter" &&
      !event.shiftKey &&
      !event.nativeEvent.isComposing
    ) {
      event.preventDefault();
      void submit();
    }
  }
  return (
    <div className="composer-lane">
      {error && (
        <div className="composer-error" role="alert">
          {error}
        </div>
      )}
      <form
        className="thread-composer"
        onSubmit={(event) => void submit(event)}
        aria-label="Send a message"
      >
        {attachment && <div className="composer-attachment">{attachment}</div>}
        <textarea
          ref={input}
          aria-label="Message"
          placeholder="Ask anything"
          value={value}
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={keyDown}
          rows={3}
          spellCheck
          autoFocus
        />
        <div className="composer-controls">
          {onSettings && (
            <button
              type="button"
              className="composer-model"
              onClick={onSettings}
              aria-label="Thread settings"
            >
              {model || "Model"}
              <span aria-hidden>⌄</span>
            </button>
          )}
          <label className="delivery-select">
            <span className="sr-only">Message delivery</span>
            <select
              value={mode}
              onChange={(event) =>
                setMode(event.target.value as "steer" | "queue")
              }
            >
              <option value="steer">Steer</option>
              <option value="queue">Queue</option>
            </select>
          </label>
          <span className="composer-spacer" />
          {onContext && (
            <button
              type="button"
              className="icon-control"
              aria-label="Add context"
              onClick={onContext}
            >
              ＋
            </button>
          )}
          {running && (
            <button
              type="button"
              className="stop-control"
              aria-label="Stop turn"
              disabled={stopping}
              onClick={() => void stop()}
            >
              {stopping ? "Stopping…" : "Stop"}
            </button>
          )}
          <button
            type="submit"
            className="send-control"
            aria-label={
              running
                ? mode === "queue"
                  ? "Queue message"
                  : "Send steering"
                : "Send message"
            }
            disabled={sending || !value.trim()}
          >
            {sending ? "…" : "↑"}
          </button>
        </div>
      </form>
      <span className="sr-only" role="status">
        {sending ? "Saving message" : running ? "Agent is working" : "Ready"}
      </span>
    </div>
  );
}
