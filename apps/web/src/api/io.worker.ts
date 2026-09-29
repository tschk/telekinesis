/// <reference lib="webworker" />
import { buildPromptExecFallback } from "./promptFallback";
import type {
  IoErrorShape,
  IoHttpErrorShape,
  IoReply,
  IoRequest,
} from "./ioProtocol";
import type {
  ExecResult,
  PromptRequest,
  PromptResult,
} from "./types";

const ctx = self as DedicatedWorkerGlobalScope;

let base = "http://127.0.0.1:8787";

function httpShape(
  status: number,
  method: string,
  path: string,
  message: string,
): IoHttpErrorShape {
  return { name: "ApiHttpError", status, method, path, message };
}

function plainShape(message: string): IoErrorShape {
  return { name: "Error", message };
}

function isHttpShape(err: unknown): err is IoHttpErrorShape {
  return (
    typeof err === "object" &&
    err !== null &&
    !(err instanceof Error) &&
    (err as IoErrorShape).name === "ApiHttpError" &&
    typeof (err as IoHttpErrorShape).message === "string"
  );
}

function isPlainShape(err: unknown): err is IoErrorShape {
  return (
    typeof err === "object" &&
    err !== null &&
    !(err instanceof Error) &&
    (err as IoErrorShape).name === "Error" &&
    typeof (err as IoErrorShape).message === "string"
  );
}

function isPromptEndpointMissing(err: unknown): boolean {
  return (
    isHttpShape(err) &&
    (err.status === 404 || err.status === 405 || err.status === 501)
  );
}

async function http<T>(method: string, path: string, body?: unknown): Promise<T> {
  let res: Response;
  try {
    const init: RequestInit = {
      method,
      headers: { "Content-Type": "application/json" },
    };
    if (body !== undefined) init.body = JSON.stringify(body);
    res = await fetch(`${base}${path}`, init);
  } catch (err) {
    const detail = err instanceof Error ? err.message : String(err);
    throw plainShape(
      `Cannot reach tk-cloud at ${base} (${method} ${path}). ${detail}`,
    );
  }
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw httpShape(
      res.status,
      method,
      path,
      `API ${method} ${path} → ${res.status}${text ? `: ${text}` : ""}`,
    );
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

/** Prompt with the exec fallback decided here, off the page thread. */
async function runPrompt(
  workspaceId: string,
  req: PromptRequest,
): Promise<PromptResult> {
  const encoded = encodeURIComponent(workspaceId);
  try {
    return await http<PromptResult>(
      "POST",
      `/v1/workspaces/${encoded}/prompt`,
      req,
    );
  } catch (err) {
    if (!isPromptEndpointMissing(err)) throw err;
    const fallback = await http<ExecResult>(
      "POST",
      `/v1/workspaces/${encoded}/exec`,
      buildPromptExecFallback(req.prompt),
    );
    return {
      ok: fallback.ok,
      text: fallback.stdout ?? fallback.message,
      stdout: fallback.stdout,
      stderr: fallback.stderr,
      message: fallback.message,
      exitCode: fallback.exitCode,
      backend: fallback.backend,
      stub: fallback.stub,
      fallback: "exec",
    };
  }
}

/** Parse the stored workspace-id blob; any parse failure yields []. */
function parseWorkspaceIds(raw: string): string[] {
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((x): x is string => typeof x === "string");
  } catch {
    return [];
  }
}

async function handle(msg: IoRequest): Promise<unknown> {
  switch (msg.type) {
    case "init":
      base = msg.baseUrl;
      return null;
    case "http":
      return http(msg.method, msg.path, msg.body);
    case "prompt":
      return runPrompt(msg.workspaceId, msg.req);
    case "parseWorkspaceIds":
      return parseWorkspaceIds(msg.raw);
    case "stringifyWorkspaceIds":
      return JSON.stringify(msg.ids);
    case "stringify":
      return JSON.stringify(msg.value);
  }
}

async function dispatch(msg: IoRequest): Promise<void> {
  const reply: IoReply = await handle(msg)
    .then((result): IoReply => ({ id: msg.id, ok: true, result }))
    .catch((err: unknown): IoReply => ({
      id: msg.id,
      ok: false,
      error: isHttpShape(err) || isPlainShape(err)
        ? err
        : plainShape(err instanceof Error ? err.message : String(err)),
    }));
  ctx.postMessage(reply);
}

ctx.onmessage = (event: MessageEvent<IoRequest>) => {
  void dispatch(event.data);
};
