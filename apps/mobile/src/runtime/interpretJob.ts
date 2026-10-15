/**
 * Pure JSON / validation jobs. No React, no fetch, no filesystem.
 *
 * Marked as a worklet so `runOnRuntimeAsync` can run it on the background
 * runtime. It is self-contained (no closed-over helpers) so Expo web can
 * replay the same source inside a Worker.
 */

export type OffThreadJob =
  | { op: "stringify"; value: unknown }
  | {
      op: "interpret";
      expect: "health" | "workspace-list" | "workspace";
      method: string;
      path: string;
      status: number;
      httpOk: boolean;
      contentType: string;
      text: string;
      /** Used when a workspace payload has no id. */
      missingIdMessage: string;
    };

export type OffThreadResult =
  | { ok: true; data: unknown }
  | { ok: false; code: "http" | "parse"; message: string; status: number | null };

type WorkspaceShape = {
  id: string;
  name: string;
  tier: string;
  createdAt: string;
  computerBackend: string | null;
  status: string;
};

export function interpretJob(job: OffThreadJob): OffThreadResult {
  "worklet";

  function isRecord(value: unknown): value is Record<string, unknown> {
    return typeof value === "object" && value !== null && !Array.isArray(value);
  }

  function looksLikeJson(text: string): boolean {
    const trimmed = text.trimStart();
    return trimmed.startsWith("{") || trimmed.startsWith("[");
  }

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
    return trimmed.length > 280 ? trimmed.slice(0, 277) + "…" : trimmed;
  }

  function describeFailure(
    method: string,
    path: string,
    status: number,
    body: string,
  ): string {
    const summary = summarizeBody(body);
    const where = method + " " + path;
    if (status === 404) return "tk-cloud " + where + " was not found (404).";
    if (status === 401 || status === 403) {
      return "tk-cloud " + where + " was not authorized (" + status + ").";
    }
    if (status === 429) return "tk-cloud " + where + " was rate-limited (429).";
    if (status >= 500) {
      return (
        "tk-cloud " +
        where +
        " failed with " +
        status +
        (summary ? ": " + summary : ".")
      );
    }
    return "tk-cloud " + where + " → " + status + (summary ? ": " + summary : "");
  }

  function parseHealth(data: unknown): { ok: boolean; status?: string } {
    if (isRecord(data)) {
      const status = typeof data.status === "string" ? data.status : undefined;
      if (typeof data.ok === "boolean") {
        const out: { ok: boolean; status?: string } = { ok: data.ok };
        if (typeof status === "string") out.status = status;
        return out;
      }
      if (status) {
        const out: { ok: boolean; status?: string } = {
          ok: /^(ok|healthy|up)$/i.test(status),
        };
        out.status = status;
        return out;
      }
      return { ok: true };
    }
    if (typeof data === "string" && data.trim()) {
      const status = data.trim();
      return { ok: /^(ok|healthy|up)$/i.test(status), status: status };
    }
    return { ok: true };
  }

  function parseWorkspaceMeta(value: unknown): WorkspaceShape | null {
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

  function parseWorkspaceList(data: unknown): WorkspaceShape[] {
    let raw: unknown[] = [];
    if (Array.isArray(data)) raw = data;
    else if (isRecord(data) && Array.isArray(data.workspaces)) {
      raw = data.workspaces;
    }
    const out: WorkspaceShape[] = [];
    for (let i = 0; i < raw.length; i++) {
      const meta = parseWorkspaceMeta(raw[i]);
      if (meta) out.push(meta);
    }
    return out;
  }

  try {
    if (job.op === "stringify") {
      const encoded = JSON.stringify(job.value);
      if (typeof encoded !== "string") {
        return {
          ok: false,
          code: "parse",
          message: "tk-cloud request body could not be encoded as JSON",
          status: null,
        };
      }
      return { ok: true, data: encoded };
    }

    const method = job.method;
    const path = job.path;

    if (!job.httpOk) {
      if (
        job.expect === "workspace-list" &&
        (job.status === 404 || job.status === 501)
      ) {
        return {
          ok: true,
          data: {
            workspaces: [],
            note:
              "GET /v1/workspaces returned " +
              job.status +
              " — list endpoint is not implemented yet; create and get-by-id may still work.",
          },
        };
      }
      return {
        ok: false,
        code: "http",
        status: job.status,
        message: describeFailure(method, path, job.status, job.text),
      };
    }

    let value: unknown = null;
    if (job.status !== 204 && job.text.trim()) {
      const contentType = job.contentType || "";
      if (
        contentType.indexOf("application/json") !== -1 ||
        looksLikeJson(job.text)
      ) {
        try {
          value = JSON.parse(job.text);
        } catch {
          return {
            ok: false,
            code: "parse",
            status: null,
            message: "tk-cloud returned invalid JSON for " + method + " " + path,
          };
        }
      } else {
        value = job.text;
      }
    }

    if (job.expect === "health") {
      return { ok: true, data: parseHealth(value) };
    }

    if (job.expect === "workspace-list") {
      return { ok: true, data: { workspaces: parseWorkspaceList(value) } };
    }

    const meta = parseWorkspaceMeta(value);
    if (!meta) {
      return {
        ok: false,
        code: "parse",
        status: null,
        message:
          job.missingIdMessage ||
          "tk-cloud workspace response was missing a workspace id",
      };
    }
    return { ok: true, data: meta };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return { ok: false, code: "parse", message: message, status: null };
  }
}
