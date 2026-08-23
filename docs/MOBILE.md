# Phone pairing (first slice)

Pair a phone to the desktop companion. The computer still runs rx4; the phone is a remote composer + timeline.

```
phone  --ws-->  apps/tunnel (Hono, loopback)  --ws-->  companion (127.0.0.1:17421)
```

## Folders

| path | role |
|---|---|
| `ui/gui` | GPUI desktop. Thin session rail, one timeline, composer, follow-up queue, model/effort chip. Loopback companion WebSocket. |
| `apps/mobile` | Same `.crepus` language, lowered with `crepuscularity-native` to View IR. Tiny SwiftUI / Compose shells consume that JSON. Not GPUI, not Electron, not Expo. |
| `apps/tunnel` | Bun + Hono. Mints a pairing token and QR-ready `tkpair://` URL, proxies `/ws/:token` to the local companion, and offers a `/signal/:token` room for SDP/ICE. |

Default `tk` features are unchanged.

## Run

```bash
# desktop companion (already binds ws://127.0.0.1:17421)
cd ui/gui && cargo run

# pairing tunnel (loopback)
cd apps/tunnel && bun install && bun start
curl -X POST http://127.0.0.1:8787/pair
```

`POST /pair` returns `token`, `url` (`tkpair://127.0.0.1:8787/ws?token=…`), `relay`, `companion`, and `iceServers`. Loopback ice is empty. `--advertise` (or `TK_TUNNEL_ADVERTISE=1`) binds `0.0.0.0` and adds a public STUN URL. There is no TURN server in this slice.

After pair, the phone speaks the companion JSON protocol (`v=1`, `op=snapshot|prompt|queue|interrupt|select|effort`) to the desktop host. The tunnel does not run the agent.

## WebRTC

The data path today is the WebSocket proxy (a documented Tailscale-shaped equivalent for this slice). `/signal/:token` is signaling only: offer / answer / candidate fan-out. A later hop can attach WebRTC using the advertised ICE list; do not treat this crate as a VPN.

## Verify

```bash
cd ui/gui && cargo test --lib --no-default-features
cd apps/tunnel && bun test
cd apps/mobile && cargo test
```
