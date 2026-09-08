# Telekinesis Mobile (M0 scaffold)

Expo TypeScript companion app for **telekinesis** — the same product surface as the TUI / GUI / web companions, aimed at on-the-go **status, steer, diffs, and approvals** once those flows land.

## Intent

- **Full-parity companion** (long-term): mirror host capabilities away from the desk — watch agent status, steer sessions, review diffs, and act on approvals.
- **Talks to private [`tschk/tk-cloud`](https://github.com/tschk/tk-cloud)** over HTTP for cloud workspaces (health + workspace list/create in M0).
- **Later:** E2EE path to a **local daemon** on the machine running telekinesis (not in M0).

## M0 scope

| Surface | Status |
|---|---|
| Cloud health (`GET /health`) | ✅ |
| List / create / get workspaces | ✅ |
| Status / Steer / Diffs / Approvals tabs | 🚧 placeholder labels only |

## Run

```bash
cd apps/mobile
npm install
npx expo start
```

Then open in Expo Go (device/simulator) or press `w` for web.

## Environment

| Variable | Default | Notes |
|---|---|---|
| `EXPO_PUBLIC_TK_CLOUD_URL` | `http://127.0.0.1:8787` | Base URL for tk-cloud. On a physical device, use your LAN IP (e.g. `http://192.168.1.10:8787`) so the phone can reach the API. |

Example:

```bash
EXPO_PUBLIC_TK_CLOUD_URL=http://192.168.1.10:8787 npx expo start
```

## API client

See `src/api/tkCloud.ts` — types mirrored from the tk-cloud protocol (`WorkspaceMeta`, `WorkspaceTier`, …). `listWorkspaces()` treats HTTP **404/501** as an empty list plus a UI note (list endpoint not ready yet).
