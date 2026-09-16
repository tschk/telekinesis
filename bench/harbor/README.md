# Harbor × Terminal-Bench 2.1 (telekinesis)

Local CLI harness bench for `tk` / `rx4` via [Harbor](https://www.harborframework.com/).  
**Not** Cloudflare Computer / orbs (those live in tk-cloud only).

See [PLAN.md](./PLAN.md) for phases, adapter design, and model lock.

## Layout

```text
bench/harbor/
  PLAN.md                      # design + phases
  README.md                    # this file
  agents/tk_harbor_agent.py    # TkHarborAgent stub (Phase C)
```

## Prerequisites (Phase A+)

| Need | Notes |
|------|--------|
| Docker | Required to *run* Harbor tasks; **not** required to land this scaffold |
| `pip install harbor` | After Docker is available on box or cp.local |
| `OPENAI_API_KEY` (or provider key) | Codex / OpenAI-backed fixed model |

This PR is **plan + stub only**. Smoke and full sweeps wait on Docker.

## Run commands (once Docker exists)

```bash
pip install harbor
export OPENAI_API_KEY=...   # Codex + any OpenAI-backed model

# Phase A — Codex smoke (3–5 task ids, k=1)
harbor run -d terminal-bench/terminal-bench-2-1 \
  --agent codex \
  -m openai/<FIXED_MODEL> \
  --task-id <id1> --task-id <id2> --task-id <id3> -k 1

# Phase C — custom tk (after adapter is fleshed out)
harbor run -d terminal-bench/terminal-bench-2-1 \
  --agent bench.harbor.agents.tk_harbor_agent:TkHarborAgent \
  -m openai/<FIXED_MODEL> \
  --ak model=openai/<FIXED_MODEL> \
  --task-id hello-world -k 1
```

Exact `--agent` import path depends on Harbor version — validate with `harbor agent list` / installed-agent API.

## Docker gate

- **This PR:** no Docker required (docs + stub only).
- **Phase A/C smoke:** blocked until Docker (or docker.sock) is available on box / cp.local.
- Do not block tk-cloud CF Computer / orbs work on this bench.

## Stub status

`agents/tk_harbor_agent.py` sketches `BaseInstalledAgent` methods (`name`, `install`, `run`, `populate_context_post_run`) without importing harbor at module load so CI stays green without the package.
