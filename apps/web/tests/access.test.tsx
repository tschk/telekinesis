// @vitest-environment jsdom
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { JSDOM } from "jsdom";
import type { WorkspaceMeta } from "../src/api/types";

const api = vi.hoisted(() => ({
  health: vi.fn(),
  list: vi.fn(),
  lookup: vi.fn(),
  prompt: vi.fn(),
  stream: vi.fn(),
}));

vi.mock("../src/api/client", () => ({
  ApiHttpError: class ApiHttpError extends Error {
    status: number;
    constructor(opts: { status: number; message: string }) {
      super(opts.message);
      this.status = opts.status;
    }
  },
  getHealth: api.health,
  listWorkspaces: api.list,
  getWorkspace: api.lookup,
  promptWorkspace: api.prompt,
  connectWorkspaceStream: api.stream,
  createWorkspace: vi.fn(),
  eventText: vi.fn(),
  promptText: vi.fn(),
  serializeValue: async (value: unknown) => JSON.stringify(value),
  parseWorkspaceIds: async (raw: string) => JSON.parse(raw),
  serializeWorkspaceIds: async (ids: string[]) => JSON.stringify(ids),
}));

import App from "../src/App";
import { ApiHttpError } from "../src/api/client";

const workspace: WorkspaceMeta = {
  id: "cached-id", name: "Private workspace", tier: "free",
  createdAt: "", computerBackend: null, status: "ready",
};
const cacheKey = "tk.workspaceIds";
let container: HTMLDivElement;
let root: Root;
let storageWindow: JSDOM;

function failure(status: number) {
  return new ApiHttpError({ status, method: "GET", path: "/", message: `HTTP ${status}` });
}

async function waitFor(assertion: () => void) {
  let error: unknown;
  for (let i = 0; i < 100; i++) {
    try {
      await act(async () => { await new Promise((resolve) => setTimeout(resolve, 5)); });
      assertion();
      return;
    } catch (err) { error = err; }
  }
  throw error;
}

function text() { return container.textContent ?? ""; }

async function mount() {
  await act(async () => { root.render(createElement(App)); });
}

async function click(label: string) {
  const button = [...container.querySelectorAll("button")].find((item) => item.textContent === label);
  if (!button) throw new Error(`Missing button: ${label}`);
  await act(async () => { button.click(); });
}

function expectDenied(status: 401 | 403) {
  expect(text()).toContain(status === 401 ? "Authentication required (401)" : "Access denied (403)");
  expect(text()).toContain("Workspace access denied");
  expect(text()).toContain("Select or create a workspace to run prompts.");
  expect(text()).not.toContain("Private workspace");
  expect(text()).not.toContain("using localStorage ids");
  expect(container.querySelector("textarea")).toBeNull();
  expect(JSON.parse(localStorage.getItem(cacheKey) ?? "null")).toEqual([workspace.id]);
}

beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  Element.prototype.scrollIntoView = () => {};
  storageWindow = new JSDOM("", { url: "https://example.test" });
  Object.defineProperty(globalThis, "localStorage", { configurable: true, value: storageWindow.window.localStorage });
  localStorage.clear();
  localStorage.setItem(cacheKey, JSON.stringify([workspace.id]));
  vi.resetAllMocks();
  api.health.mockResolvedValue({ ok: true });
  api.list.mockResolvedValue([workspace]);
  api.lookup.mockResolvedValue(workspace);
  api.stream.mockReturnValue({ close: vi.fn() });
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => { root.unmount(); });
  container.remove();
  storageWindow.window.close();
});

describe("workspace authorization", () => {
  it.each([401, 403] as const)("does not expose cached workspaces when listing returns %s", async (status) => {
    api.list.mockRejectedValue(failure(status));
    await mount();
    await waitFor(() => expectDenied(status));
  });

  it.each([401, 403] as const)("does not expose cached workspaces when fallback lookup returns %s", async (status) => {
    api.list.mockRejectedValue(failure(503));
    api.lookup.mockRejectedValue(failure(status));
    await mount();
    await waitFor(() => expectDenied(status));
  });

  it("shows authentication failure from health without restoring cached selection", async () => {
    api.health.mockRejectedValue(failure(401));
    await mount();
    await waitFor(() => expectDenied(401));
    expect(text()).toContain("API down");
  });

  it("clears selection and prompt log when a prompt is unauthorized", async () => {
    api.prompt.mockRejectedValue(failure(401));
    await mount();
    await waitFor(() => expect(text()).toContain("Prompt · Private workspace"));
    const input = container.querySelector("textarea");
    if (!input) throw new Error("Missing prompt input");
    await act(async () => {
      const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set;
      setter?.call(input, "secret prompt");
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await click("Send");
    await waitFor(() => expectDenied(401));
    expect(text()).not.toContain("secret prompt");
  });

  it("uses and selects cached workspaces when listing is unavailable", async () => {
    api.list.mockRejectedValue(failure(503));
    await mount();
    await waitFor(() => expect(text()).toContain("Prompt · Private workspace"));
    expect(text()).toContain("List API unavailable — using localStorage ids");
    expect(container.querySelector("textarea")).not.toBeNull();
    expect(JSON.parse(localStorage.getItem(cacheKey) ?? "null")).toEqual([workspace.id]);
  });

  it("ignores an older successful refresh after a later authorization denial", async () => {
    await mount();
    await waitFor(() => expect(text()).toContain("Prompt · Private workspace"));
    let resolveRefresh: (value: WorkspaceMeta[]) => void = () => { throw new Error("Missing refresh"); };
    api.list.mockImplementationOnce(() => new Promise<WorkspaceMeta[]>((resolve) => { resolveRefresh = resolve; }));
    await click("Refresh");
    api.list.mockRejectedValueOnce(failure(403));
    await click("Refresh");
    await waitFor(() => expectDenied(403));
    await act(async () => { resolveRefresh([workspace]); });
    expectDenied(403);
  });

  it("shows authentication failure when an unavailable stream's workspace probe is unauthorized", async () => {
    api.lookup.mockRejectedValue(failure(401));
    let unavailable: (() => void) | undefined;
    api.stream.mockImplementation((options: { onUnavailable: () => void }) => {
      unavailable = options.onUnavailable;
      return { close: vi.fn() };
    });
    await mount();
    await waitFor(() => expect(text()).toContain("Prompt · Private workspace"));
    if (!unavailable) throw new Error("Missing stream");
    await act(async () => { unavailable?.(); });
    await waitFor(() => expectDenied(401));
  });

  it("distinguishes an ambiguous socket failure and checks access again on reconnect", async () => {
    api.lookup.mockResolvedValueOnce(workspace).mockRejectedValueOnce(failure(401));
    let status: ((value: string) => void) | undefined;
    let unavailable: (() => void) | undefined;
    api.stream.mockImplementation((options: { onStatus: (value: string) => void; onUnavailable: () => void }) => {
      status = options.onStatus;
      unavailable = options.onUnavailable;
      return { close: vi.fn() };
    });
    await mount();
    await waitFor(() => expect(text()).toContain("Prompt · Private workspace"));
    if (!unavailable || !status) throw new Error("Missing stream callbacks");
    await act(async () => { unavailable?.(); });
    await waitFor(() => expect(text()).toContain("unavailable — check connection or access"));
    expect(text()).toContain("Private workspace");
    await act(async () => { status?.("open"); status?.("reconnecting"); });
    await waitFor(() => expectDenied(401));
  });
});
