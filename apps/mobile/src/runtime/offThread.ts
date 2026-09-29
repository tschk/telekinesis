/**
 * Off-thread runner for the Expo mobile companion.
 *
 * Mechanism (Expo SDK 57 / RN 0.86): `react-native-worklets` 0.10.1 — the
 * version bundled with Expo Go — `createWorkletRuntime` + `runOnRuntimeAsync`.
 * `runOnRuntime` / `scheduleOnRuntime` only enqueue work and do not return a
 * value. `runOnRuntimeSync` blocks the caller, so it is not used.
 *
 * Worklet runtimes in 0.10 ship `performance.now` and `console` only.
 * `fetch` on a background runtime (`enableNetworking`) arrived in worklets
 * 0.13, which is not the Expo SDK 57 native module. HTTP therefore uses async
 * `fetch` + `response.text()` here, on the native networking stack, started
 * from this module (never from render). JSON.parse, JSON.stringify, and
 * response validation run only inside `interpretJob` on the background
 * runtime. The RN JS thread receives a finished `{ ok, data | error }`.
 *
 * Expo web: `createWorkletRuntime` throws. The same interpreter runs in a
 * dedicated Web Worker — a real background thread, not `setTimeout(0)` and
 * not `InteractionManager`.
 *
 * Config: `EXPO_PUBLIC_TK_CLOUD_URL` is inlined by Metro and read once in
 * `baseUrl()`. There is no sync file read on the render path. If persistence
 * is added later, read bytes with async `expo-file-system` and pass the
 * string through `runOffThread` — do not parse or read files while painting.
 */
import { Platform } from "react-native";
import {
  createWorkletRuntime,
  runOnRuntimeAsync,
  type WorkletRuntime,
} from "react-native-worklets";
import {
  interpretJob,
  type OffThreadJob,
  type OffThreadResult,
} from "./interpretJob";

export type { OffThreadJob, OffThreadResult };

const RUNTIME_NAME = "tk-mobile-bg";

let backgroundRuntime: WorkletRuntime | null = null;

function getBackgroundRuntime(): WorkletRuntime {
  if (!backgroundRuntime) {
    backgroundRuntime = createWorkletRuntime({ name: RUNTIME_NAME });
  }
  return backgroundRuntime;
}

type WorkerWaiter = {
  resolve: (value: OffThreadResult) => void;
  reject: (error: Error) => void;
};

type WebMessage = {
  id?: number;
  ok?: boolean;
  value?: OffThreadResult;
  message?: string;
};

type WebWorkerLike = {
  postMessage: (data: unknown) => void;
  onmessage: ((event: { data: WebMessage }) => void) | null;
  onerror: ((event: { message?: string }) => void) | null;
};

let webWorker: WebWorkerLike | null = null;
let webWorkerSeq = 1;
const webWaiters = new Map<number, WorkerWaiter>();

function rejectAllWebWaiters(error: Error): void {
  const waiters = Array.from(webWaiters.values());
  webWaiters.clear();
  for (let i = 0; i < waiters.length; i++) {
    waiters[i].reject(error);
  }
}

function ensureWebWorker(): WebWorkerLike {
  if (webWorker) return webWorker;
  const scope = globalThis as unknown as {
    Worker?: new (url: string) => WebWorkerLike;
    Blob?: new (parts: string[], options: { type: string }) => object;
    URL?: { createObjectURL: (blob: object) => string };
  };
  if (!scope.Worker || !scope.Blob || !scope.URL) {
    throw new Error(
      "Expo web has no Worker, and react-native-worklets runtimes are native-only.",
    );
  }
  const source =
    "var interpretJob = " +
    interpretJob.toString() +
    ";\n" +
    "self.onmessage = function (event) {\n" +
    "  var id = event.data && event.data.id;\n" +
    "  try {\n" +
    "    var value = interpretJob(event.data.job);\n" +
    "    self.postMessage({ id: id, ok: true, value: value });\n" +
    "  } catch (err) {\n" +
    "    var message = err && err.message ? String(err.message) : String(err);\n" +
    "    self.postMessage({ id: id, ok: false, message: message });\n" +
    "  }\n" +
    "};\n";
  const blob = new scope.Blob([source], { type: "application/javascript" });
  const worker = new scope.Worker(scope.URL.createObjectURL(blob));
  worker.onmessage = (event) => {
    const data = event.data;
    if (!data || typeof data.id !== "number") return;
    const waiter = webWaiters.get(data.id);
    if (!waiter) return;
    webWaiters.delete(data.id);
    if (!data.ok || !data.value) {
      waiter.reject(new Error(data.message || "off-thread worker failed"));
      return;
    }
    waiter.resolve(data.value);
  };
  worker.onerror = (event) => {
    rejectAllWebWaiters(new Error(event.message || "off-thread worker error"));
    webWorker = null;
  };
  webWorker = worker;
  return worker;
}

function runOnWebWorker(job: OffThreadJob): Promise<OffThreadResult> {
  const worker = ensureWebWorker();
  const id = webWorkerSeq++;
  return new Promise((resolve, reject) => {
    webWaiters.set(id, { resolve, reject });
    worker.postMessage({ id: id, job: job });
  });
}

/** Parse, stringify, or validate `job` off the UI JS thread. */
export function runOffThread(job: OffThreadJob): Promise<OffThreadResult> {
  if (Platform.OS === "web") {
    return runOnWebWorker(job);
  }
  return runOnRuntimeAsync(getBackgroundRuntime(), interpretJob, job);
}

export type HttpReadResult =
  | {
      kind: "response";
      status: number;
      httpOk: boolean;
      contentType: string;
      text: string;
    }
  | { kind: "timeout" }
  | { kind: "aborted" }
  | { kind: "network"; detail: string };

function isAbortLike(err: unknown): boolean {
  return (
    typeof err === "object" &&
    err !== null &&
    "name" in err &&
    (err as { name?: string }).name === "AbortError"
  );
}

/**
 * Native networking stack. Async `fetch` + body read, never called from
 * render. Does not parse JSON — the body string is handed to `runOffThread`.
 */
export async function readHttpResponse(opts: {
  url: string;
  method: string;
  headers: Record<string, string>;
  body?: string;
  signal?: AbortSignal;
  timeoutMs: number;
}): Promise<HttpReadResult> {
  const controller = new AbortController();
  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    controller.abort();
  }, opts.timeoutMs);

  const external = opts.signal;
  const onExternalAbort = () => {
    controller.abort();
  };
  if (external) {
    if (external.aborted) controller.abort();
    else external.addEventListener("abort", onExternalAbort);
  }

  try {
    const res = await fetch(opts.url, {
      method: opts.method,
      signal: controller.signal,
      headers: opts.headers,
      body: opts.body,
    });
    let text = "";
    try {
      text = await res.text();
    } catch {
      text = "";
    }
    return {
      kind: "response",
      status: res.status,
      httpOk: res.ok,
      contentType: res.headers.get("content-type") ?? "",
      text,
    };
  } catch (err) {
    if (isAbortLike(err) || controller.signal.aborted) {
      if (timedOut && !(external && external.aborted)) return { kind: "timeout" };
      return { kind: "aborted" };
    }
    const detail = err instanceof Error ? err.message : String(err);
    return { kind: "network", detail };
  } finally {
    clearTimeout(timer);
    if (external) external.removeEventListener("abort", onExternalAbort);
  }
}
