/**
 * Workspace event stream.
 *
 * Connects to `GET /v1/workspaces/:id/stream?after=<id>`, replays missed
 * rows, then keeps the live tail. On drop, reconnects from the last id
 * so a hibernated socket does not skip or duplicate a row.
 *
 * Payloads are opaque. Pi JSONL v3 entries pass through; this module does
 * not call a model and does not attach provider credentials.
 */

export interface WorkspaceEvent {
  id: number;
  ts: string;
  kind: string;
  payload: unknown;
}

export interface EventPage {
  events: WorkspaceEvent[];
  latest: number;
}

export interface WorkspaceStream {
  close(): void;
  /** Last applied event id. 0 before the first row. */
  cursor(): number;
  /** Queue a control event. Flushed once the socket is open. */
  send(kind: "steer" | "approval_response", payload: Record<string, unknown>): void;
}

export interface ConnectWorkspaceStreamOptions {
  baseUrl: string;
  workspaceId: string;
  after?: number;
  /** Override for tests. Defaults to global WebSocket. */
  WebSocket?: typeof globalThis.WebSocket;
  onEvents: (events: WorkspaceEvent[]) => void;
  onStatus?: (status: "connecting" | "open" | "reconnecting" | "closed") => void;
  /** Missing stream endpoint (HTTP fallback also 404/426). Stop instead of looping. */
  onUnavailable?: () => void;
}

const MAX_BACKOFF_MS = 15_000;
/** Matches tk-cloud MAX_EVENT_PAGE. A full page is not the end of the log. */
const MAX_EVENT_PAGE = 500;

export function connectWorkspaceStream(
  opts: ConnectWorkspaceStreamOptions,
): WorkspaceStream {
  const WS = opts.WebSocket ?? globalThis.WebSocket;
  let cursor = normalizeCursor(opts.after);
  let closed = false;
  let socket: WebSocket | null = null;
  let attempt = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;
  const pending: string[] = [];

  const flush = (ws: WebSocket) => {
    if (ws.readyState !== ws.OPEN) return;
    while (pending.length > 0) {
      ws.send(pending.shift()!);
    }
  };

  const connect = () => {
    if (closed) return;
    opts.onStatus?.(attempt === 0 ? "connecting" : "reconnecting");
    const url = streamUrl(opts.baseUrl, opts.workspaceId, cursor);
    let opened = false;
    let ws: WebSocket;
    try {
      ws = new WS(url);
    } catch {
      schedule();
      return;
    }
    socket = ws;

    ws.addEventListener("open", () => {
      opened = true;
      attempt = 0;
      opts.onStatus?.("open");
      flush(ws);
    });

    ws.addEventListener("message", (ev) => {
      const page = parsePage(ev.data);
      if (!page) return;
      const fresh = page.events.filter((event) => event.id > cursor);
      if (fresh.length === 0) {
        if (page.latest > cursor) cursor = page.latest;
        return;
      }
      fresh.sort((a, b) => a.id - b.id);
      cursor = fresh[fresh.length - 1]!.id;
      opts.onEvents(fresh);
      // The server sends one page per socket. A full page means older rows
      // remain; reconnect from the new cursor instead of waiting for live frames.
      if (fresh.length >= MAX_EVENT_PAGE) {
        ws.close();
      }
    });

    ws.addEventListener("close", () => {
      if (socket === ws) socket = null;
      if (closed) return;
      // A socket that never opened is usually a missing endpoint, not a blip.
      if (!opened && attempt === 0) {
        opts.onUnavailable?.();
      }
      schedule();
    });

    ws.addEventListener("error", () => {
      // close follows; reconnect happens there.
    });
  };

  const schedule = () => {
    if (closed) return;
    attempt += 1;
    const delay = Math.min(MAX_BACKOFF_MS, 500 * 2 ** (attempt - 1));
    timer = setTimeout(connect, delay);
  };

  connect();

  return {
    cursor: () => cursor,
    send: (kind, payload) => {
      if (closed) return;
      pending.push(JSON.stringify({ kind, payload }));
      if (socket) flush(socket);
    },
    close: () => {
      closed = true;
      if (timer) clearTimeout(timer);
      opts.onStatus?.("closed");
      socket?.close();
      socket = null;
    },
  };
}

export function streamUrl(baseUrl: string, workspaceId: string, after: number): string {
  const base = stripTrailingSlashes(baseUrl.trim() || "http://127.0.0.1:8787");
  const http = new URL(
    `/v1/workspaces/${encodeURIComponent(workspaceId)}/stream?after=${normalizeCursor(after)}`,
    base.endsWith("/") ? base : `${base}/`,
  );
  http.protocol = http.protocol === "https:" ? "wss:" : "ws:";
  return http.toString();
}

/** Text of a pi message entry, if the payload is one. Otherwise null. */
export function eventText(event: WorkspaceEvent): string | null {
  const payload = event.payload;
  if (!isRecord(payload)) return null;
  if (typeof payload.content === "string" && payload.content.trim()) {
    return payload.content;
  }
  if (typeof payload.text === "string" && payload.text.trim()) return payload.text;
  if (typeof payload.summary === "string" && payload.summary.trim()) {
    return payload.summary;
  }
  return null;
}

function parsePage(data: unknown): EventPage | null {
  const text = typeof data === "string" ? data : null;
  if (text == null) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return null;
  }
  if (!isRecord(parsed) || !Array.isArray(parsed.events)) return null;
  const events: WorkspaceEvent[] = [];
  for (const item of parsed.events) {
    if (!isRecord(item)) continue;
    if (typeof item.id !== "number" || !Number.isFinite(item.id)) continue;
    events.push({
      id: item.id,
      ts: typeof item.ts === "string" ? item.ts : "",
      kind: typeof item.kind === "string" ? item.kind : "custom",
      payload: item.payload,
    });
  }
  const latest =
    typeof parsed.latest === "number" && Number.isFinite(parsed.latest)
      ? parsed.latest
      : 0;
  return { events, latest };
}

function normalizeCursor(after: number | undefined): number {
  if (after == null || !Number.isFinite(after) || after < 0) return 0;
  return Math.floor(after);
}

function stripTrailingSlashes(url: string): string {
  return url.replace(/\/+$/, "");
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
