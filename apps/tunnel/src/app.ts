import { getConnInfo } from "hono/bun";
import { Hono } from "hono";
import { createBunWebSocket } from "hono/bun";
import { iceServers } from "./ice.ts";
import { isLoopbackHost } from "./listen.ts";
import { PairStore, pairUrl } from "./pair.ts";
import { isSignalMessage, SignalRoom } from "./signal.ts";

export type TunnelOptions = {
  host: string;
  port: number;
  advertise: boolean;
  companionWs: string;
  store?: PairStore;
  allowCompanionProxy?: boolean;
  pairFromLoopbackOnly?: boolean;
  maxWsQueue?: number;
};

export type TunnelApp = {
  app: Hono;
  websocket: ReturnType<typeof createBunWebSocket>["websocket"];
  store: PairStore;
};

const { upgradeWebSocket, websocket } = createBunWebSocket();
const DEFAULT_WS_QUEUE = 32;

function pathToken(value: string | undefined): string {
  return value ?? "";
}

function clientIsLocal(c: { req: unknown }, publicHost: string): boolean {
  try {
    const info = getConnInfo(c as never);
    const address = info.remote.address ?? "";
    return isLoopbackHost(address) || address === publicHost;
  } catch {
    return false;
  }
}

export function createTunnel(options: TunnelOptions): TunnelApp {
  const store = options.store ?? new PairStore();
  const rooms = new Map<string, SignalRoom>();
  const app = new Hono();
  const allowCompanionProxy = options.allowCompanionProxy ?? !options.advertise;
  const pairFromLoopbackOnly = options.pairFromLoopbackOnly ?? options.advertise;
  const maxWsQueue = options.maxWsQueue ?? DEFAULT_WS_QUEUE;

  const dropEmptyRoom = (token: string) => {
    const room = rooms.get(token);
    if (!room || room.size() === 0) {
      rooms.delete(token);
    }
  };

  const pruneRooms = () => {
    store.prune();
    for (const token of [...rooms.keys()]) {
      if (!store.get(token)) {
        rooms.delete(token);
      } else {
        dropEmptyRoom(token);
      }
    }
  };

  app.get("/health", (c) => c.json({ ok: true, advertise: options.advertise }));

  app.get("/ice", (c) => c.json({ iceServers: iceServers(options.advertise) }));

  app.post("/pair", (c) => {
    pruneRooms();
    if (pairFromLoopbackOnly && !clientIsLocal(c, options.host)) {
      return c.json({ error: "pair minting is local-only" }, 403);
    }
    const record = store.mint();
    const url = pairUrl(options.host, options.port, record.token);
    return c.json({
      token: record.token,
      expiresAt: record.expiresAt,
      url,
      relay: allowCompanionProxy
        ? `ws://${options.host}:${options.port}/ws/${record.token}`
        : `ws://${options.host}:${options.port}/signal/${record.token}`,
      companion: allowCompanionProxy ? options.companionWs : undefined,
      iceServers: iceServers(options.advertise),
      advertise: options.advertise,
    });
  });

  app.get("/pair/:token", (c) => {
    pruneRooms();
    const record = store.get(c.req.param("token"));
    if (!record) {
      return c.json({ error: "invalid or expired pair token" }, 404);
    }
    return c.json({
      token: record.token,
      expiresAt: record.expiresAt,
      companion: allowCompanionProxy ? options.companionWs : undefined,
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
          pruneRooms();
          if (!token || !store.get(token) || !allowCompanionProxy) {
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
        onMessage(event, ws) {
          const data = String(event.data);
          if (!upstream || upstream.readyState !== WebSocket.OPEN) {
            if (queue.length >= maxWsQueue) {
              ws.close();
              return;
            }
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
          pruneRooms();
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
          dropEmptyRoom(token);
        },
      };
    }),
  );

  return { app, websocket, store };
}
