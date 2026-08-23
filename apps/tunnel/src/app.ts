import { Hono } from "hono";
import { createBunWebSocket } from "hono/bun";
import { iceServers } from "./ice.ts";
import { PairStore, pairUrl } from "./pair.ts";
import { isSignalMessage, SignalRoom } from "./signal.ts";

export type TunnelOptions = {
  host: string;
  port: number;
  advertise: boolean;
  companionWs: string;
  store?: PairStore;
};

export type TunnelApp = {
  app: Hono;
  websocket: ReturnType<typeof createBunWebSocket>["websocket"];
  store: PairStore;
};

const { upgradeWebSocket, websocket } = createBunWebSocket();

function pathToken(value: string | undefined): string {
  return value ?? "";
}

export function createTunnel(options: TunnelOptions): TunnelApp {
  const store = options.store ?? new PairStore();
  const rooms = new Map<string, SignalRoom>();
  const app = new Hono();

  app.get("/health", (c) => c.json({ ok: true, advertise: options.advertise }));

  app.get("/ice", (c) => c.json({ iceServers: iceServers(options.advertise) }));

  app.post("/pair", (c) => {
    const record = store.mint();
    const url = pairUrl(options.host, options.port, record.token);
    return c.json({
      token: record.token,
      expiresAt: record.expiresAt,
      url,
      relay: `ws://${options.host}:${options.port}/ws/${record.token}`,
      companion: options.companionWs,
      iceServers: iceServers(options.advertise),
      advertise: options.advertise,
    });
  });

  app.get("/pair/:token", (c) => {
    const record = store.get(c.req.param("token"));
    if (!record) {
      return c.json({ error: "invalid or expired pair token" }, 404);
    }
    return c.json({
      token: record.token,
      expiresAt: record.expiresAt,
      companion: options.companionWs,
    });
  });

  app.get(
    "/ws/:token",
    upgradeWebSocket((c) => {
      const token = pathToken(c.req.param("token"));
      let upstream: WebSocket | undefined;
      const queue: string[] = [];
      return {
        onOpen(_event, ws) {
          if (!token || !store.get(token)) {
            ws.close();
            return;
          }
          upstream = new WebSocket(options.companionWs);
          upstream.addEventListener("open", () => {
            for (const item of queue) {
              upstream?.send(item);
            }
            queue.length = 0;
          });
          upstream.addEventListener("message", (event) => {
            ws.send(String(event.data));
          });
          upstream.addEventListener("close", () => ws.close());
        },
        onMessage(event) {
          const data = String(event.data);
          if (!upstream || upstream.readyState !== WebSocket.OPEN) {
            queue.push(data);
            return;
          }
          upstream.send(data);
        },
        onClose() {
          upstream?.close();
        },
      };
    }),
  );

  app.get(
    "/signal/:token",
    upgradeWebSocket((c) => {
      const token = pathToken(c.req.param("token"));
      return {
        onOpen(_event, ws) {
          if (!token || !store.get(token)) {
            ws.close();
            return;
          }
          const room = rooms.get(token) ?? new SignalRoom();
          rooms.set(token, room);
          room.add(ws);
        },
        onMessage(event, ws) {
          let parsed: unknown;
          try {
            parsed = JSON.parse(String(event.data));
          } catch {
            return;
          }
          if (!isSignalMessage(parsed)) {
            return;
          }
          rooms.get(token)?.fanout(ws, parsed);
        },
        onClose(_event, ws) {
          rooms.get(token)?.remove(ws);
        },
      };
    }),
  );

  return { app, websocket, store };
}
