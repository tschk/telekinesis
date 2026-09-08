# Telekinesis Web (`apps/web`)

Cloud control-plane UI for Telekinesis workspaces. React + Vite on Cloudflare Pages; talks to private [`tschk/tk-cloud`](https://github.com/tschk/tk-cloud).

## Dev

```bash
# terminal A — cloud API
cd /path/to/tk-cloud && npm i && npm run dev   # http://127.0.0.1:8787

# terminal B — this app
cd apps/web
cp .env.example .env.local   # optional
npm i
npm run dev
```

Env:

| Variable | Default | Purpose |
|---|---|---|
| `VITE_API_BASE_URL` | `http://127.0.0.1:8787` | tk-cloud Worker |
| `VITE_BILLING_PORTAL_URL` | `#` stub | Stripe portal (M2) |

## UI kit tokens

Shared CSS contract with `apps/desktop` (canonical values mirrored 1:1):

`src/styles/tokens.css` — `--tk-*` colors, space, radius, fonts.

Dark samples: `--tk-bg #0b0f12`, `--tk-surface #12181d`, `--tk-accent #2dd4bf`.

## MVP surfaces

- Health badge (`GET /health`)
- Workspace cards (`GET /v1/workspaces`, localStorage fallback)
- Create workspace (`POST /v1/workspaces`)
- Prompt → exec + log (`POST /v1/workspaces/:id/exec`)
- Diff pane stub, billing portal link stub

## Build / Pages

```bash
npm run build   # → dist/
npx wrangler pages deploy dist
```

## License

Same as monorepo root (MPL-2.0).
