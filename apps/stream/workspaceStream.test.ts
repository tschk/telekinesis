import { describe, expect, it, vi } from "vitest";
import { connectWorkspaceStream, eventText, streamUrl } from "./workspaceStream";

class FakeSocket {
  static instances: FakeSocket[] = [];
  url: string;
  listeners = new Map<string, ((ev: { data?: unknown }) => void)[]>();
  closed = false;

  constructor(url: string) {
    this.url = url;
    FakeSocket.instances.push(this);
  }

  addEventListener(type: string, fn: (ev: { data?: unknown }) => void) {
    const list = this.listeners.get(type) ?? [];
    list.push(fn);
    this.listeners.set(type, list);
  }

  emit(type: string, data?: unknown) {
    for (const fn of this.listeners.get(type) ?? []) fn({ data });
  }

  close() {
    this.closed = true;
    this.emit("close");
  }
}

describe("streamUrl", () => {
  it("switches https to wss and keeps a fractional cursor from skipping a row", () => {
    expect(streamUrl("https://cloud.tk.tsc.hk/", "ws/1", 4.9)).toBe(
      "wss://cloud.tk.tsc.hk/v1/workspaces/ws%2F1/stream?after=4",
    );
  });

  it("uses ws for a local wrangler dev base", () => {
    expect(streamUrl("http://127.0.0.1:8787", "abc", 0)).toBe(
      "ws://127.0.0.1:8787/v1/workspaces/abc/stream?after=0",
    );
  });
});

describe("connectWorkspaceStream", () => {
  it("applies replay then live, and reconnects after the last id", () => {
    vi.useFakeTimers();
    FakeSocket.instances = [];
    const seen: number[] = [];
    const stream = connectWorkspaceStream({
      baseUrl: "http://127.0.0.1:8787",
      workspaceId: "abc",
      WebSocket: FakeSocket as unknown as typeof WebSocket,
      onEvents: (events) => {
        for (const event of events) seen.push(event.id);
      },
    });

    const first = FakeSocket.instances[0]!;
    first.emit("open");
    first.emit(
      "message",
      JSON.stringify({
        events: [
          { id: 1, ts: "t", kind: "message", payload: { content: "one" } },
          { id: 2, ts: "t", kind: "steer", payload: { text: "two" } },
        ],
        latest: 2,
      }),
    );
    first.emit(
      "message",
      JSON.stringify({
        events: [{ id: 2, ts: "t", kind: "steer", payload: { text: "two" } }],
        latest: 2,
      }),
    );
    expect(seen).toEqual([1, 2]);
    expect(stream.cursor()).toBe(2);

    first.emit("close");
    vi.advanceTimersByTime(500);
    const second = FakeSocket.instances[1]!;
    expect(second.url).toContain("after=2");
    second.emit("open");
    second.emit(
      "message",
      JSON.stringify({
        events: [
          { id: 2, ts: "t", kind: "steer", payload: { text: "dup" } },
          { id: 3, ts: "t", kind: "message", payload: { content: "three" } },
        ],
        latest: 3,
      }),
    );
    expect(seen).toEqual([1, 2, 3]);

    stream.close();
    expect(second.closed).toBe(true);
    vi.useRealTimers();
  });

  it("reconnects immediately when a page is full so the rest of the log is not skipped", () => {
    vi.useFakeTimers();
    FakeSocket.instances = [];
    connectWorkspaceStream({
      baseUrl: "http://127.0.0.1:8787",
      workspaceId: "abc",
      WebSocket: FakeSocket as unknown as typeof WebSocket,
      onEvents: () => {},
    });
    const first = FakeSocket.instances[0]!;
    const events = Array.from({ length: 500 }, (_, i) => ({
      id: i + 1,
      ts: "t",
      kind: "message",
      payload: { content: String(i + 1) },
    }));
    first.emit("open");
    first.emit("message", JSON.stringify({ events, latest: 500 }));
    expect(first.closed).toBe(true);
    vi.advanceTimersByTime(500);
    expect(FakeSocket.instances[1]?.url).toContain("after=500");
    vi.useRealTimers();
  });

  it("does not apply an out-of-order live frame ahead of a gap", () => {
    FakeSocket.instances = [];
    const seen: number[] = [];
    connectWorkspaceStream({
      baseUrl: "http://127.0.0.1:8787",
      workspaceId: "abc",
      after: 5,
      WebSocket: FakeSocket as unknown as typeof WebSocket,
      onEvents: (events) => {
        for (const event of events) seen.push(event.id);
      },
    });
    FakeSocket.instances[0]!.emit(
      "message",
      JSON.stringify({
        events: [
          { id: 7, ts: "t", kind: "message", payload: {} },
          { id: 6, ts: "t", kind: "message", payload: {} },
        ],
        latest: 7,
      }),
    );
    expect(seen).toEqual([6, 7]);
  });
});

describe("eventText", () => {
  it("reads pi content, steer text, and compaction summary", () => {
    expect(eventText({ id: 1, ts: "", kind: "message", payload: { content: "hi" } })).toBe(
      "hi",
    );
    expect(eventText({ id: 1, ts: "", kind: "steer", payload: { text: "stop" } })).toBe(
      "stop",
    );
    expect(
      eventText({ id: 1, ts: "", kind: "compaction", payload: { summary: "done" } }),
    ).toBe("done");
    expect(eventText({ id: 1, ts: "", kind: "custom", payload: { n: 1 } })).toBeNull();
  });
});
