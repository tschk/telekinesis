/** tk-cloud HTTP client + protocol types (mirrored; no package dep). */

import { readHttpResponse, runOffThread } from "../runtime/offThread";


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

/**
 * Agent prompt turn (web parity; mirrors in-progress tk-cloud
 * `POST /v1/workspaces/:id/prompt`). Types only — no client call yet.
 */
export interface PromptRequest {
  prompt: string;
  model?: string;
  cwd?: string;
}

export interface PromptResult {
  ok: boolean;
  text?: string;
  output?: string;
  stdout?: string;
  stderr?: string;
  message?: string;
  exitCode?: number;
  backend?: ComputerBackend | string;
  stub?: boolean;
  fallback?: "exec";
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
  return CLOUD_BASE_URL;
}

/**
 * Metro inlines `EXPO_PUBLIC_*` at bundle time. Read once at module init so
 * render only receives the finished string (no file or env work per paint).
 */
const CLOUD_BASE_URL = readCloudBaseUrl();

function readCloudBaseUrl(): string {
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

type Expectation = "health" | "workspace-list" | "workspace";

async function callCloud<T>(
  path: string,
  expect: Expectation,
  init: RequestInit & TkCloudRequestOptions & { jsonBody?: unknown } = {},
  missingIdMessage = "",
): Promise<T> {
  const timeoutOpt = init.timeoutMs;
  const external = init.signal;
  const initHeaders = init.headers;
  const initMethod = init.method;
  const jsonBody = init.jsonBody;
  const method = (initMethod ?? "GET").toUpperCase();
  const timeoutMs = timeoutOpt ?? DEFAULT_TIMEOUT_MS;

  if (external?.aborted) {
    throw new TkCloudError({
      code: "aborted",
      method,
      path,
      message: `tk-cloud request aborted (${method} ${path})`,
    });
  }

  let bodyText: string | undefined;
  if (jsonBody !== undefined) {
    const encoded = await runOffThread({ op: "stringify", value: jsonBody });
    if (!encoded.ok || typeof encoded.data !== "string") {
      throw new TkCloudError({
        code: "parse",
        method,
        path,
        message: encoded.ok
          ? `tk-cloud request body could not be encoded as JSON (${method} ${path})`
          : encoded.message,
      });
    }
    bodyText = encoded.data;
  }

  if (external?.aborted) {
    throw new TkCloudError({
      code: "aborted",
      method,
      path,
      message: `tk-cloud request aborted (${method} ${path})`,
    });
  }

  const headers = mergeHeaders(
    {
      Accept: "application/json",
      ...(bodyText !== undefined ? { "Content-Type": "application/json" } : {}),
    },
    initHeaders,
  );

  const read = await readHttpResponse({
    url: apiUrl(path),
    method,
    headers,
    body: bodyText,
    signal: external,
    timeoutMs,
  });

  if (read.kind === "timeout") {
    throw new TkCloudError({
      code: "timeout",
      method,
      path,
      message: `tk-cloud timed out after ${timeoutMs / 1000}s (${method} ${path}). Is it running at ${baseUrl()}?`,
    });
  }
  if (read.kind === "aborted") {
    throw new TkCloudError({
      code: "aborted",
      method,
      path,
      message: `tk-cloud request aborted (${method} ${path})`,
    });
  }
  if (read.kind === "network") {
    throw new TkCloudError({
      code: "network",
      method,
      path,
      message: `Cannot reach tk-cloud at ${baseUrl()} (${method} ${path}). Check EXPO_PUBLIC_TK_CLOUD_URL. ${read.detail}`,
    });
  }

  const finished = await runOffThread({
    op: "interpret",
    expect,
    method,
    path,
    status: read.status,
    httpOk: read.httpOk,
    contentType: read.contentType,
    text: read.text,
    missingIdMessage,
  });

  if (!finished.ok) {
    throw new TkCloudError({
      code: finished.code,
      method,
      path,
      status: finished.status === null ? undefined : finished.status,
      message: finished.message,
    });
  }
  return finished.data as T;
}

export async function getHealth(
  options?: TkCloudRequestOptions,
): Promise<HealthResponse> {
  return callCloud<HealthResponse>("/health", "health", options);
}

/**
 * List workspaces. HTTP 404/501 → empty list + note (list endpoint not ready).
 * Other non-OK statuses throw.
 */
export async function listWorkspaces(
  options?: TkCloudRequestOptions,
): Promise<ListWorkspacesResult> {
  return callCloud<ListWorkspacesResult>("/v1/workspaces", "workspace-list", options);
}

export async function createWorkspace(
  body?: CreateWorkspaceRequest,
  options?: TkCloudRequestOptions,
): Promise<WorkspaceMeta> {
  return callCloud<WorkspaceMeta>(
    "/v1/workspaces",
    "workspace",
    {
      ...options,
      method: "POST",
      jsonBody: body ?? {},
    },
    "tk-cloud create workspace response was missing a workspace id",
  );
}

export async function getWorkspace(
  id: string,
  options?: TkCloudRequestOptions,
): Promise<WorkspaceMeta> {
  const path = `/v1/workspaces/${encodeURIComponent(id)}`;
  return callCloud<WorkspaceMeta>(
    path,
    "workspace",
    options,
    `tk-cloud workspace ${id} response was missing a workspace id`,
  );
}
