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
- **Cloud:** list / create workspaces + exec via `VITE_TK_CLOUD_API` (tk-cloud M1); graceful mock fallback **only** on network/HTTP failure (empty API list is valid)
- **Local:** “Spawn telekinesis” invokes `tk` or `telekinesis` on `PATH` via a Tauri command; missing binary is reported clearly (not required to build)
- **Layout stubs:** AppShell (top bar + left nav + main), WorkspaceCard, StatusBadge, Panel stubs for terminal / diff / in-app browser; Cloud PromptBox + LogPane for `/exec`
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
- Optional (Cloud): local tk-cloud wrangler dev from [tk-cloud PR #1](https://github.com/tschk/tk-cloud/pull/1) (`m1-computer-isolate-shell`)

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
# .env / .env.local — point at wrangler dev for tk-cloud PR #1
# https://github.com/tschk/tk-cloud/pull/1  (branch m1-computer-isolate-shell)
VITE_TK_CLOUD_API=http://127.0.0.1:8787
```

Default is `http://127.0.0.1:8787`. M1 routes used by this shell:

| Method | Path | Notes |
|---|---|---|
| `GET` | `/health` | `{ ok, service, env, computer }` — health chip |
| `GET` | `/v1/workspaces` | bare `WorkspaceMeta[]` (empty array is valid) |
| `POST` | `/v1/workspaces` | `{ name?, tier? }` → 201 `WorkspaceMeta` |
| `GET` | `/v1/workspaces/:id` | `WorkspaceMeta` |
| `POST` | `/v1/workspaces/:id/exec` | `{ source, backend?, cwd? }` → `ExecResult` |

`WorkspaceMeta`: `{ id, name, tier, createdAt, computerBackend, status }`.

## License

MPL-2.0 (same as telekinesis)
