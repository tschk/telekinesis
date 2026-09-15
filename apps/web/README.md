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
- Prompt → agent + log (`POST /v1/workspaces/:id/prompt`)
- Diff pane stub, billing portal link stub

### Prompt endpoint + exec fallback

`PromptBox` submits via `promptWorkspace()` in `src/api/client.ts`:

1. `POST /v1/workspaces/:id/prompt` with `{ prompt, model?, cwd? }`.
   No API keys are sent from the frontend — prompts go to tk-cloud and
   model keys stay on the server/workspace.
2. If that endpoint is not implemented yet (404/405/501), the client falls
   back to `POST /v1/workspaces/:id/exec` with a shell-quoted
   `tk exec '<prompt>'` command string and marks the result
   `fallback: "exec"` (shown in the log as
   `(via exec fallback: POST /prompt not implemented yet)`).
   The tk-cloud Worker itself is not implemented here.

Agent text is buffered (streaming/SSE is a future enhancement) and rendered
in `LogPane` via `promptText()`, which accepts `text`/`output`/`stdout`/
`message` while the server shape lands. If the API is down, the page keeps
the localStorage workspace-id fallback and shows a clear
`Cannot reach tk-cloud at <base> …` error in the log.

## Build / Pages

```bash
npm run build   # → dist/
npx wrangler pages deploy dist
```

## License

Same as monorepo root (MPL-2.0).
