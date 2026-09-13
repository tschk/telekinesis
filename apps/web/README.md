# Telekinesis Web (`apps/web`)

Monorepo ADE / web control GUI companion for Telekinesis workspaces (React + Vite).

**Not `cloud.tk.tsc.hk`.** That hostname is the tk-cloud portal
([`tschk/tk-cloud`](https://github.com/tschk/tk-cloud) `apps/web`, moonshine + crepus).
This app lives in the telekinesis monorepo for local/dev and Pages deploy elsewhere — it does not own or deploy to `cloud.tk.tsc.hk`.

## Visual alignment

Matches the portal vibe (not the host):

- Font: Chivo Mono (Google Fonts)
- Palette: zinc-950 surfaces (`#09090b` …), sparse mono layout
- Shared CSS tokens with `apps/desktop`: `src/styles/tokens.css` (`--tk-*` names, portal-aligned values)

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
