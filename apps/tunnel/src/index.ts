import { createTunnel } from "./app.ts";

const advertise = process.argv.includes("--advertise") || process.env.TK_TUNNEL_ADVERTISE === "1";
const port = Number(process.env.TK_TUNNEL_PORT ?? 8787);
const host = advertise ? (process.env.TK_ADVERTISE_HOST ?? "0.0.0.0") : "127.0.0.1";
const publicHost = process.env.TK_ADVERTISE_HOST ?? (advertise ? host : "127.0.0.1");
const companionWs = process.env.TK_COMPANION_WS ?? "ws://127.0.0.1:17421";

const { app, websocket } = createTunnel({
  host: publicHost === "0.0.0.0" ? "127.0.0.1" : publicHost,
  port,
  advertise,
  companionWs,
});

const listenHost = advertise ? "0.0.0.0" : "127.0.0.1";

export default {
  port,
  hostname: listenHost,
  fetch: app.fetch,
  websocket,
};

if (import.meta.main) {
  const server = Bun.serve({
    port,
    hostname: listenHost,
    fetch: app.fetch,
    websocket,
  });
  console.log(
    `telekinesis tunnel ${server.hostname}:${server.port} companion=${companionWs} advertise=${advertise}`,
  );
}
