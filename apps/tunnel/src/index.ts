import { createTunnel } from "./app.ts";
import { resolveListenConfig, withCompanionToken } from "./listen.ts";

const advertise = process.argv.includes("--advertise") || process.env.TK_TUNNEL_ADVERTISE === "1";
const port = Number(process.env.TK_TUNNEL_PORT ?? 8787);
const companionToken = process.env.TK_COMPANION_TOKEN;
const companionWs = withCompanionToken(
  process.env.TK_COMPANION_WS ?? "ws://127.0.0.1:17421",
  companionToken,
);

const listen = resolveListenConfig({
  advertise,
  advertiseHost: process.env.TK_ADVERTISE_HOST,
});

const { app, websocket } = createTunnel({
  host: listen.publicHost,
  port,
  advertise: listen.advertise,
  companionWs,
  allowCompanionProxy: !listen.advertise,
  pairFromLoopbackOnly: listen.advertise,
});

if (import.meta.main) {
  const server = Bun.serve({
    port,
    hostname: listen.bindHost,
    fetch: app.fetch,
    websocket,
  });
  console.log(
    `telekinesis tunnel ${server.hostname}:${server.port} companion=${companionWs} advertise=${listen.advertise} pairHost=${listen.publicHost}`,
  );
}
