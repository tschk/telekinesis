# Phone pairing (first slice)

Pair a phone to the desktop companion. The computer still runs rx4; the phone is a remote composer + timeline.

```
phone  --ws-->  apps/tunnel (Hono)  --ws-->  companion (127.0.0.1:17421)
```

Loopback tunnel mode proxies `/ws/:token` to the companion. Advertise mode never proxies a public listener onto the loopback companion; phones use `/signal/:token` instead.

## Folders

| path | role |
|---|---|
| `ui/gui` | GPUI desktop. Thin session rail, one timeline, composer, follow-up queue, model/effort chip. Authenticated loopback companion WebSocket. |
| `apps/mobile` | Same `.crepus` language, lowered with `crepuscularity-native` to View IR. Tiny SwiftUI / Compose shells consume that JSON. Not GPUI, not Electron, not Expo. |
| `apps/tunnel` | Bun + Hono. Mints a pairing token and QR-ready `tkpair://` URL, proxies `/ws/:token` to the local companion on loopback, and offers a `/signal/:token` room for SDP/ICE. |

Default `tk` features are unchanged.

## Run

```bash
# desktop companion (binds ws://127.0.0.1:17421, requires a companion token)
cd ui/gui && cargo run

# pairing tunnel (loopback)
export TK_COMPANION_TOKEN=...   # same value the companion loaded
cd apps/tunnel && bun install && bun start
curl -X POST http://127.0.0.1:8787/pair
```

The companion reads `TK_COMPANION_TOKEN` or writes one to `~/.telekinesis/companion.token`. WebSocket clients must present that token (`?token=` or `Authorization: Bearer`) and are rejected when the Origin is not local.

`POST /pair` returns `token`, `url` (`tkpair://127.0.0.1:8787/ws?token=…`), `relay`, `companion`, and `iceServers`. Loopback ice is empty. Minting is local-only when advertising.

`--advertise` (or `TK_TUNNEL_ADVERTISE=1`) binds the reachable LAN host from `TK_ADVERTISE_HOST` or the first non-loopback IPv4 interface — not `0.0.0.0` — and adds a public STUN URL. Pairing URLs never use `127.0.0.1`. There is no TURN server in this slice.

After pair, the phone speaks the companion JSON protocol (`v=1`, `op=snapshot|prompt|queue|interrupt|select|effort`) to the desktop host. The tunnel does not run the agent.

## WebRTC

The data path today is the WebSocket proxy on loopback. `/signal/:token` is signaling only: offer / answer / candidate fan-out. A later hop can attach WebRTC using the advertised ICE list; do not treat this crate as a VPN.

## Verify

```bash
cd ui/gui && cargo test --lib --no-default-features
cd apps/tunnel && bun test
cd apps/mobile && cargo test
```
