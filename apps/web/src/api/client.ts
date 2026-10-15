import type {
  CreateWorkspaceRequest,
  ExecRequest,
  ExecResult,
  PromptRequest,
  PromptResult,
  WorkspaceMeta,
} from "./types";
import type { IoReply, IoRequest } from "./ioProtocol";
import { buildPromptExecFallback } from "./promptFallback";

const DEFAULT_BASE = "http://127.0.0.1:8787";

function baseUrl(): string {
  const raw = import.meta.env.VITE_API_BASE_URL as string | undefined;
  return (raw && raw.trim()) || DEFAULT_BASE;
}

/** HTTP error with the status attached so callers can branch on 404/405/501. */
export class ApiHttpError extends Error {
  readonly status: number;
  readonly method: string;
  readonly path: string;

  constructor(opts: { status: number; method: string; path: string; message: string }) {
    super(opts.message);
    this.name = "ApiHttpError";
    this.status = opts.status;
    this.method = opts.method;
    this.path = opts.path;
  }
}

/**
 * All IO and JSON run in a module worker; this module is only a promise
 * proxy on the page. The base URL is read here (workers do not see
 * import.meta.env the same way) and handed to the worker in the first
 * message; later calls queue until init is acknowledged.
 */
const worker = new Worker(new URL("./io.worker.ts", import.meta.url), {
  type: "module",
});

let nextId = 1;
const pending = new Map<
  number,
  { resolve: (value: unknown) => void; reject: (err: unknown) => void }
>();

let acknowledgeInit: () => void;
const initAcknowledged = new Promise<void>((resolve) => {
  acknowledgeInit = resolve;
});

worker.onmessage = (event: MessageEvent<IoReply>) => {
  const reply = event.data;
  const entry = pending.get(reply.id);
  if (!entry) return;
  pending.delete(reply.id);
  if (reply.ok) {
    entry.resolve(reply.result);
    return;
  }
  if (reply.error.name === "ApiHttpError") {
    entry.reject(
      new ApiHttpError({
        status: reply.error.status,
        method: reply.error.method,
        path: reply.error.path,
        message: reply.error.message,
      }),
    );
    return;
  }
  entry.reject(new Error(reply.error.message));
};

function freshId(): number {
  const id = nextId;
  nextId += 1;
  return id;
}

function request<T>(message: IoRequest): Promise<T> {
  return initAcknowledged.then(
    () =>
      new Promise<T>((resolve, reject) => {
        pending.set(message.id, {
          resolve: resolve as (value: unknown) => void,
          reject: reject as (err: unknown) => void,
        });
        worker.postMessage(message);
      }),
  );
}

const initId = freshId();
pending.set(initId, {
  resolve: () => acknowledgeInit(),
  reject: () => acknowledgeInit(),
});
worker.postMessage({ id: initId, type: "init", baseUrl: baseUrl() });

export async function getHealth(): Promise<{ ok?: boolean; status?: string } | unknown> {
  return request({ id: freshId(), type: "http", method: "GET", path: "/health" });
}

export async function listWorkspaces(): Promise<WorkspaceMeta[]> {
  return request<WorkspaceMeta[]>({
    id: freshId(),
    type: "http",
    method: "GET",
    path: "/v1/workspaces",
  });
}

export async function createWorkspace(
  body?: CreateWorkspaceRequest,
): Promise<WorkspaceMeta> {
  return request<WorkspaceMeta>({
    id: freshId(),
    type: "http",
    method: "POST",
    path: "/v1/workspaces",
    body: body ?? {},
  });
}

export async function getWorkspace(id: string): Promise<WorkspaceMeta> {
  return request<WorkspaceMeta>({
    id: freshId(),
    type: "http",
    method: "GET",
    path: `/v1/workspaces/${encodeURIComponent(id)}`,
  });
}

export async function exec(
  id: string,
  req: ExecRequest,
): Promise<ExecResult> {
  return request<ExecResult>({
    id: freshId(),
    type: "http",
    method: "POST",
    path: `/v1/workspaces/${encodeURIComponent(id)}/exec`,
    body: req,
  });
}

/**
 * Send an agent prompt to a workspace. The 404/405/501 → exec fallback
 * runs inside the worker, so the page sends one request and receives the
 * finished result (including `fallback: "exec"` markers).
 */
export async function promptWorkspace(
  id: string,
  req: PromptRequest,
): Promise<PromptResult> {
  return request<PromptResult>({
    id: freshId(),
    type: "prompt",
    workspaceId: id,
    req,
  });
}

/** Parse the stored workspace-id blob in the worker; failures yield []. */
export async function parseWorkspaceIds(raw: string): Promise<string[]> {
  return request<string[]>({ id: freshId(), type: "parseWorkspaceIds", raw });
}

/** Serialize the workspace-id list in the worker. */
export async function serializeWorkspaceIds(ids: string[]): Promise<string> {
  return request<string>({ id: freshId(), type: "stringifyWorkspaceIds", ids });
}

/** Off-thread serialization for display fallbacks (e.g. log lines). */
export async function serializeValue(value: unknown): Promise<string> {
  return request<string>({ id: freshId(), type: "stringify", value });
}

export { buildPromptExecFallback };

/** True when the prompt endpoint itself is missing (not a prompt failure). */
export function isPromptEndpointMissing(err: unknown): boolean {
  return (
    err instanceof ApiHttpError &&
    (err.status === 404 || err.status === 405 || err.status === 501)
  );
}

/**
 * Buffered agent text from a prompt result. Accepts `text`/`output`/
 * `stdout`/`message` because the prompt endpoint is still landing
 * server-side and shapes may vary.
 */
export function promptText(result: PromptResult): string | undefined {
  for (const field of [result.text, result.output, result.stdout, result.message]) {
    if (typeof field === "string" && field.trim()) return field;
  }
  return undefined;
}

export { baseUrl };
