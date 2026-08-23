import { describe, expect, test } from "bun:test";
import { createTunnel } from "../src/app.ts";

function serveCompanionEcho() {
  return Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    fetch(req, server) {
      if (server.upgrade(req)) {
        return undefined as never;
      }
      return new Response("companion", { status: 426 });
    },
    websocket: {
      message(ws, message) {
        ws.send(`echo:${String(message)}`);
      },
    },
  });
}

describe("tunnel http + ws proxy", () => {
  test("mints a pair and proxies websocket to the companion", async () => {
    const companion = serveCompanionEcho();
    const { app, websocket, store } = createTunnel({
      host: "127.0.0.1",
      port: 0,
      advertise: false,
      companionWs: `ws://127.0.0.1:${companion.port}`,
    });
    const tunnel = Bun.serve({
      hostname: "127.0.0.1",
      port: 0,
      fetch: app.fetch,
      websocket,
    });

    const minted = await fetch(`http://127.0.0.1:${tunnel.port}/pair`, { method: "POST" });
    expect(minted.status).toBe(200);
    const body = (await minted.json()) as { token: string; url: string; advertise: boolean };
    expect(body.token).toHaveLength(32);
    expect(body.url).toContain(`token=${body.token}`);
    expect(body.advertise).toBe(false);
    expect(store.get(body.token)).toBeDefined();

    const missing = await fetch(`http://127.0.0.1:${tunnel.port}/pair/not-a-token`);
    expect(missing.status).toBe(404);

    const reply = await new Promise<string>((resolve, reject) => {
      const ws = new WebSocket(`ws://127.0.0.1:${tunnel.port}/ws/${body.token}`);
      ws.addEventListener("open", () => ws.send('{"v":1,"op":"snapshot"}'));
      ws.addEventListener("message", (event) => resolve(String(event.data)));
      ws.addEventListener("error", () => reject(new Error("ws error")));
      setTimeout(() => reject(new Error("ws timeout")), 3_000);
    });
    expect(reply).toBe('echo:{"v":1,"op":"snapshot"}');

    const rejected = await new Promise<number>((resolve) => {
      const ws = new WebSocket(`ws://127.0.0.1:${tunnel.port}/ws/bad-token`);
      ws.addEventListener("close", (event) => resolve(event.code));
      ws.addEventListener("error", () => resolve(0));
    });
    expect(rejected).not.toBeUndefined();

    tunnel.stop();
    companion.stop();
  });
});

  test("does not proxy companion websocket when advertising", async () => {
    const companion = serveCompanionEcho();
    const { app, websocket } = createTunnel({
      host: "192.168.1.20",
      port: 0,
      advertise: true,
      companionWs: `ws://127.0.0.1:${companion.port}`,
      allowCompanionProxy: false,
      pairFromLoopbackOnly: false,
    });
    const tunnel = Bun.serve({
      hostname: "127.0.0.1",
      port: 0,
      fetch: app.fetch,
      websocket,
    });
    const minted = await fetch(`http://127.0.0.1:${tunnel.port}/pair`, { method: "POST" });
    const body = (await minted.json()) as { token: string; companion?: string; url: string; relay: string };
    expect(body.companion).toBeUndefined();
    expect(body.url).not.toContain("127.0.0.1");
    expect(body.relay).toContain("/signal/");

    const closed = await new Promise<boolean>((resolve) => {
      const ws = new WebSocket(`ws://127.0.0.1:${tunnel.port}/ws/${body.token}`);
      ws.addEventListener("open", () => ws.send("should-not-echo"));
      ws.addEventListener("message", () => resolve(false));
      ws.addEventListener("close", () => resolve(true));
      setTimeout(() => resolve(true), 500);
    });
    expect(closed).toBe(true);

    tunnel.stop();
    companion.stop();
  });
