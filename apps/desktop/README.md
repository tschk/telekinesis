# apps/desktop — Telekinesis ADE shell

Experimental **desktop ADE shell** for Telekinesis (Tauri 2 + React + TypeScript + Vite).

Lives **inside** [`tschk/telekinesis`](https://github.com/tschk/telekinesis) at `apps/desktop`. The archived sibling `tschk/tk-desktop` is **not** the product home.

Related surfaces:

| Surface | Path / repo | Role |
|---|---|---|
| TUI | `ui/tui` | **Primary** agent surface (ratatui) |
| GPUI companion | `ui/gui` (`telekinesis-companion`) | Experimental native GPUI window — **do not replace** |
| ADE desktop (this app) | `apps/desktop` | Parallel worktrees / terminals / diffs / in-app browser / Local↔Cloud host switcher |
| Web control plane | `apps/web` (relocating from tk-web) | React control plane; shared `--tk-*` CSS tokens |
| Cloud API | private [`tschk/tk-cloud`](https://github.com/tschk/tk-cloud) | Workers / Workspace Durable Objects |

Competitors / inspiration for the ADE shell: Orca, Emdash, Superset.

## MVP

- **Host switcher:** Local | Cloud (top bar)
- **Cloud:** list workspaces via `VITE_TK_CLOUD_API` (`GET /v1/workspaces`); graceful fallback to in-app mock JSON
- **Local:** “Spawn telekinesis” invokes `tk` or `telekinesis` on `PATH` via a Tauri command; missing binary is reported clearly (not required to build)
- **Layout stubs:** AppShell (top bar + left nav + main), WorkspaceCard, StatusBadge, Panel stubs for terminal / diff / in-app browser
- **Tokens:** `src/styles/tokens.css` — exact `--tk-*` names shared with the web contract (dark-first, teal/cyan on near-black)

## Architecture

```
apps/desktop ──► Local: telekinesis CLI/daemon (`tk` / `telekinesis`)
             └─► Cloud: tk-cloud API + Workspace Durable Objects
```

Local = free ADE on your machine. Cloud = always-on host when the laptop sleeps.

## Prerequisites

- Node 20+
- Rust (stable) + [Tauri prerequisites](https://tauri.app/start/prerequisites/)
- Optional: `tk` / `telekinesis` on `PATH` (`cargo install telekinesis` or repo `install.sh`)

## Run

From this directory (`apps/desktop`):

```bash
npm install
npm run tauri dev
```

Frontend-only (no Tauri IPC / spawn):

```bash
npm run dev
```

Cloud API base URL (optional):

```bash
# .env / .env.local
VITE_TK_CLOUD_API=http://127.0.0.1:8787
```

Default is `http://127.0.0.1:8787`. Expected list shape:

`{ "workspaces": [ { "id", "name", "status", "region?", "updatedAt?" } ] }` or a bare array.

## License

MPL-2.0 (same as telekinesis)
