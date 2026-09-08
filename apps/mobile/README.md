# Telekinesis Mobile (M0)

Expo TypeScript companion for **telekinesis**. Long-term this is full-parity remote control: status, steer, diffs, approvals, and push when an agent is blocked or done. M0 talks HTTP to private [`tschk/tk-cloud`](https://github.com/tschk/tk-cloud). A later milestone adds an E2EE path to a local telekinesis daemon.

## Run

```bash
cd apps/mobile
npm install
npx expo start
```

Then Expo Go (device/simulator) or `w` for web. Metro must be restarted after changing env vars — Expo inlines `EXPO_PUBLIC_*` at start.

## Environment

| Variable | Default | Notes |
|---|---|---|
| `EXPO_PUBLIC_TK_CLOUD_URL` | `http://127.0.0.1:8787` | tk-cloud base URL. Whitespace and trailing slashes are stripped. On a **physical device**, `127.0.0.1` is the phone — use the machine LAN IP, e.g. `http://192.168.1.10:8787`. |

```bash
EXPO_PUBLIC_TK_CLOUD_URL=http://192.168.1.10:8787 npx expo start
```

No API keys or other secrets in this app. Requests time out after 15s.

## M0 scope

| Surface | Status |
|---|---|
| Cloud health (`GET /health`) | Wired |
| Create workspace (`POST /v1/workspaces`) | Wired |
| Get workspace (`GET /v1/workspaces/:id`) | Client helper only |
| List workspaces (`GET /v1/workspaces`) | Wired, with fallback below |
| Status / Steer / Diffs / Approvals | Placeholder labels |

## Missing `GET /v1/workspaces`

tk-cloud may not implement the list endpoint yet. The client treats HTTP **404** and **501** as a successful empty list and shows a UI note:

> GET /v1/workspaces returned 404/501 — list endpoint is not implemented yet; create and get-by-id may still work.

Any other non-OK status is an error. Workspaces created in this session are kept on screen when list returns that fallback (so a successful `POST` is not wiped). Pull to refresh retries health + list against the same base URL.

## API client

`src/api/tkCloud.ts` — protocol types mirrored from tk-cloud (`WorkspaceMeta`, `WorkspaceTier`, …). No package dependency on the private repo.
