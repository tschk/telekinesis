/** tk-cloud HTTP client + protocol types (mirrored; no package dep). */

export type WorkspaceTier = "free" | "pro" | "team";

export type ComputerBackend = "isolate-shell" | "container" | "isolate-js";

export interface WorkspaceMeta {
  id: string;
  name: string;
  tier: WorkspaceTier | string;
  createdAt: string;
  computerBackend: ComputerBackend | string | null;
  status: string;
}

export interface CreateWorkspaceRequest {
  name?: string;
  tier?: WorkspaceTier | string;
}

export interface ListWorkspacesResult {
  workspaces: WorkspaceMeta[];
  /** Present when GET /v1/workspaces returned 404/501 (endpoint not ready). */
  note?: string;
}

export interface HealthResponse {
  ok: boolean;
  status?: string;
}

export type TkCloudErrorCode =
  | "http"
  | "network"
  | "timeout"
  | "aborted"
  | "parse";

export class TkCloudError extends Error {
  readonly code: TkCloudErrorCode;
  readonly method: string;
  readonly path: string;
  readonly status?: number;

  constructor(opts: {
    code: TkCloudErrorCode;
    message: string;
    method: string;
    path: string;
    status?: number;
  }) {
    super(opts.message);
    this.name = "TkCloudError";
    this.code = opts.code;
    this.method = opts.method;
    this.path = opts.path;
    this.status = opts.status;
  }
}

export function isTkCloudError(err: unknown): err is TkCloudError {
  return err instanceof TkCloudError;
}

/** Caller-aborted request (unmount / superseded fetch) — not a user-facing error. */
export function isAbortError(err: unknown): boolean {
  return isTkCloudError(err) && err.code === "aborted";
}

export function formatTkCloudError(err: unknown): string {
  if (isTkCloudError(err) || err instanceof Error) return err.message;
  return String(err);
}

export const DEFAULT_TIMEOUT_MS = 15_000;
const DEFAULT_BASE = "http://127.0.0.1:8787";

export type TkCloudRequestOptions = {
  signal?: AbortSignal;
  timeoutMs?: number;
};

/** Trim whitespace and trailing slashes so `${base}/v1/...` never doubles up. */
export function baseUrl(): string {
  const raw = process.env.EXPO_PUBLIC_TK_CLOUD_URL;
  const trimmed = typeof raw === "string" ? raw.trim() : "";
  return stripTrailingSlashes(trimmed || DEFAULT_BASE);
}

function stripTrailingSlashes(url: string): string {
  return url.replace(/\/+$/, "");
}

function apiUrl(path: string): string {
  const suffix = path.startsWith("/") ? path : `/${path}`;
  return `${baseUrl()}${suffix}`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function looksLikeJson(text: string): boolean {
  const t = text.trimStart();
  return t.startsWith("{") || t.startsWith("[");
}

function isAbortLike(err: unknown): boolean {
  return (
    typeof err === "object" &&
    err !== null &&
    "name" in err &&
    (err as { name?: string }).name === "AbortError"
  );
}

/** Prefer JSON `error` / `message`; otherwise a capped raw snippet. */
function summarizeBody(text: string): string {
  const trimmed = text.trim();
  if (!trimmed) return "";
  try {
    const parsed: unknown = JSON.parse(trimmed);
    if (isRecord(parsed)) {
      if (typeof parsed.error === "string" && parsed.error.trim()) {
        return parsed.error.trim();
      }
      if (typeof parsed.message === "string" && parsed.message.trim()) {
        return parsed.message.trim();
      }
    }
  } catch {
    // use raw text
  }
  return trimmed.length > 280 ? `${trimmed.slice(0, 277)}…` : trimmed;
}

function parseJson(text: string, method: string, path: string): unknown {
  try {
    return JSON.parse(text) as unknown;
  } catch {
    throw new TkCloudError({
      code: "parse",
      method,
      path,
      message: `tk-cloud returned invalid JSON for ${method} ${path}`,
    });
  }
}

async function readText(res: Response): Promise<string> {
  try {
    return await res.text();
  } catch {
    return "";
  }
}

function describeFailure(
  method: string,
  path: string,
  status: number,
  body: string,
): string {
  const summary = summarizeBody(body);
  const where = `${method} ${path}`;
  if (status === 404) return `tk-cloud ${where} was not found (404).`;
  if (status === 401 || status === 403) {
    return `tk-cloud ${where} was not authorized (${status}).`;
  }
  if (status === 429) return `tk-cloud ${where} was rate-limited (429).`;
  if (status >= 500) {
    return `tk-cloud ${where} failed with ${status}${summary ? `: ${summary}` : "."}`;
  }
  return `tk-cloud ${where} → ${status}${summary ? `: ${summary}` : ""}`;
}

function mergeHeaders(
  base: Record<string, string>,
  extra?: HeadersInit,
): Record<string, string> {
  if (!extra) return base;
  const out = { ...base };
  if (Array.isArray(extra)) {
    for (const pair of extra) {
      out[pair[0]] = pair[1];
    }
  } else if (typeof Headers !== "undefined" && extra instanceof Headers) {
    extra.forEach((value, key) => {
      out[key] = value;
    });
  } else {
    for (const [key, value] of Object.entries(extra)) {
      if (typeof value === "string") out[key] = value;
    }
  }
  return out;
}

async function request(
  path: string,
  init: RequestInit & TkCloudRequestOptions = {},
): Promise<unknown> {
  const {
    timeoutMs: timeoutOpt,
    signal: external,
    headers: initHeaders,
    method: initMethod,
    ...rest
  } = init;
  const method = (initMethod ?? "GET").toUpperCase();
  const timeoutMs = timeoutOpt ?? DEFAULT_TIMEOUT_MS;
  const url = apiUrl(path);

  const controller = new AbortController();
  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    controller.abort();
  }, timeoutMs);

  const onExternalAbort = () => {
    controller.abort();
  };
  if (external) {
    if (external.aborted) controller.abort();
    else external.addEventListener("abort", onExternalAbort);
  }

  const headers = mergeHeaders(
    {
      Accept: "application/json",
      ...(rest.body != null ? { "Content-Type": "application/json" } : {}),
    },
    initHeaders,
  );

  try {
    const res = await fetch(url, {
      ...rest,
      method,
      signal: controller.signal,
      headers,
    });
    const text = await readText(res);

    if (!res.ok) {
      throw new TkCloudError({
        code: "http",
        method,
        path,
        status: res.status,
        message: describeFailure(method, path, res.status, text),
      });
    }

    if (res.status === 204 || !text.trim()) return undefined;

    const contentType = res.headers.get("content-type") ?? "";
    if (contentType.includes("application/json") || looksLikeJson(text)) {
      return parseJson(text, method, path);
    }
    return text;
  } catch (err) {
    if (isTkCloudError(err)) throw err;
    if (isAbortLike(err) || controller.signal.aborted) {
      if (timedOut && !external?.aborted) {
        throw new TkCloudError({
          code: "timeout",
          method,
          path,
          message: `tk-cloud timed out after ${timeoutMs / 1000}s (${method} ${path}). Is it running at ${baseUrl()}?`,
        });
      }
      throw new TkCloudError({
        code: "aborted",
        method,
        path,
        message: `tk-cloud request aborted (${method} ${path})`,
      });
    }
    const detail = err instanceof Error ? err.message : String(err);
    throw new TkCloudError({
      code: "network",
      method,
      path,
      message: `Cannot reach tk-cloud at ${baseUrl()} (${method} ${path}). Check EXPO_PUBLIC_TK_CLOUD_URL. ${detail}`,
    });
  } finally {
    clearTimeout(timer);
    external?.removeEventListener("abort", onExternalAbort);
  }
}

function parseHealth(data: unknown): HealthResponse {
  if (isRecord(data)) {
    const status = typeof data.status === "string" ? data.status : undefined;
    if (typeof data.ok === "boolean") return { ok: data.ok, status };
    if (status) return { ok: /^(ok|healthy|up)$/i.test(status), status };
    return { ok: true, status };
  }
  if (typeof data === "string" && data.trim()) {
    const status = data.trim();
    return { ok: /^(ok|healthy|up)$/i.test(status), status };
  }
  // 2xx with empty/unknown body still means the process answered.
  return { ok: true };
}

function parseWorkspaceMeta(value: unknown): WorkspaceMeta | null {
  if (!isRecord(value) || typeof value.id !== "string" || !value.id.trim()) {
    return null;
  }
  const backend = value.computerBackend;
  return {
    id: value.id,
    name:
      typeof value.name === "string" && value.name.trim()
        ? value.name
        : value.id,
    tier: typeof value.tier === "string" && value.tier ? value.tier : "free",
    createdAt: typeof value.createdAt === "string" ? value.createdAt : "",
    computerBackend: typeof backend === "string" && backend ? backend : null,
    status:
      typeof value.status === "string" && value.status ? value.status : "unknown",
  };
}

function parseWorkspaceList(data: unknown): WorkspaceMeta[] {
  let raw: unknown[] = [];
  if (Array.isArray(data)) raw = data;
  else if (isRecord(data) && Array.isArray(data.workspaces)) {
    raw = data.workspaces;
  }

  const out: WorkspaceMeta[] = [];
  for (const item of raw) {
    const meta = parseWorkspaceMeta(item);
    if (meta) out.push(meta);
  }
  return out;
}

export async function getHealth(
  options?: TkCloudRequestOptions,
): Promise<HealthResponse> {
  return parseHealth(await request("/health", options));
}

/**
 * List workspaces. HTTP 404/501 → empty list + note (list endpoint not ready).
 * Other non-OK statuses throw.
 */
export async function listWorkspaces(
  options?: TkCloudRequestOptions,
): Promise<ListWorkspacesResult> {
  try {
    const data = await request("/v1/workspaces", options);
    return { workspaces: parseWorkspaceList(data) };
  } catch (err) {
    if (isTkCloudError(err) && (err.status === 404 || err.status === 501)) {
      return {
        workspaces: [],
        note: `GET /v1/workspaces returned ${err.status} — list endpoint is not implemented yet; create and get-by-id may still work.`,
      };
    }
    throw err;
  }
}

export async function createWorkspace(
  body?: CreateWorkspaceRequest,
  options?: TkCloudRequestOptions,
): Promise<WorkspaceMeta> {
  const data = await request("/v1/workspaces", {
    ...options,
    method: "POST",
    body: JSON.stringify(body ?? {}),
  });
  const meta = parseWorkspaceMeta(data);
  if (!meta) {
    throw new TkCloudError({
      code: "parse",
      method: "POST",
      path: "/v1/workspaces",
      message: "tk-cloud create workspace response was missing a workspace id",
    });
  }
  return meta;
}

export async function getWorkspace(
  id: string,
  options?: TkCloudRequestOptions,
): Promise<WorkspaceMeta> {
  const path = `/v1/workspaces/${encodeURIComponent(id)}`;
  const data = await request(path, options);
  const meta = parseWorkspaceMeta(data);
  if (!meta) {
    throw new TkCloudError({
      code: "parse",
      method: "GET",
      path,
      message: `tk-cloud workspace ${id} response was missing a workspace id`,
    });
  }
  return meta;
}
