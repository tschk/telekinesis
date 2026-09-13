# Harbor × Terminal-Bench 2.1 — TK / rotary harness bench

Owner: tschk (orchestrate) · bulk via omp/grok  
Status: design draft (2026-09-13)  
Constraint: **do not block** tk-cloud CF Computer/orbs (M1/M2).  
Orbs live in **tk-cloud only** — this bench is **local CLI harness** (`tk` / `rx4`), not Cloudflare Computer.

## Goal

Primary metric: **Terminal-Bench 2.1 pass@1 %** via [Harbor](https://www.harborframework.com/).

Compare agents **with model held fixed**:

| Agent | Harbor role | Notes |
|-------|-------------|--------|
| Codex CLI | builtin | Prefer `OPENAI_API_KEY` (ChatGPT sub ≠ always API) |
| OpenCode / omp | custom or builtin if present | Same model id as Codex run |
| **Custom `tk` / rx4** | **custom installed agent** | Headless `tk exec` over rotary host |
| Amp | **Phase E** | Not in Harbor builtins today |

Dataset: `terminal-bench/terminal-bench-2-1`  
Smoke: `-k 1` on 3–5 task ids before full sweep.

## Phases

| Phase | What | Gate |
|-------|------|------|
| **A** | Docker + `pip install harbor`; smoke 3–5 tasks × Codex (baseline) | Docker available on box/cp.local |
| **B** | OpenCode/omp Harbor agent (or `--agent` builtin if listed) same model | Codex baseline % recorded |
| **C** | **Custom `TkHarborAgent`** (installed) wrapping `tk exec` | Adapter green on `hello-world`-class task |
| **D** | Full TB2.1 pass@1 table (Codex vs omp vs tk) fixed model | Cost/time budget from Max |
| **E** | Amp adapter (if/when Harbor support or external agent) | After D |

**Current box (2026-09-13):** no `docker` / no docker.sock → Phase A blocked until Docker or move smoke to cp.local.

## Adapter design — `TkHarborAgent` (installed)

Prefer **installed** agent (agent binary + tools inside task container), matching Codex/Claude pattern and localharness TB2 Harbor adapter.

### Contract (Harbor `BaseInstalledAgent`)

```text
name()        -> "tk" | "telekinesis"
install(env)  -> install `tk` binary + rotary-compatible config into container
run(instruction, env, context) -> headless agent loop
populate_context_post_run(context) -> parse trajectory / logs
```

### Install strategy

1. **Release binary** (preferred): curl `tk` from telekinesis GitHub releases / `install.sh` into agent user PATH.  
2. **Fallback**: copy prebuilt artifact from Harbor job host mount (`--ak binary_path=...`).  
3. Write minimal config under `~/.config/telekinesis` or env:
   - Provider/model from Harbor `-m` / `--ak model=...` (must match Codex run).  
   - `AlwaysAllow` / yolo for non-TTY (matches `tk exec` default).  
4. Inject secrets via Harbor env (never bake into image): `OPENAI_API_KEY` / provider keys as required by the fixed model.

### Run strategy

Map Harbor task instruction →:

```bash
tk exec --json --cwd "$TASK_WORKDIR" --model "$MODEL" "$INSTRUCTION"
# or stdin:
printf '%s\n' "$INSTRUCTION" | tk exec --json --cwd "$TASK_WORKDIR" --model "$MODEL" -
```

- Tee agent stdout/stderr to `/logs/agent/tk.txt` (Harbor convention).  
- Timeout: leave ~30s under Harbor agent timeout (e.g. 870s if 900s limit) so verifier can run.  
- Rotary host events (recovery / spill) already on telekinesis main (`#22`) — log them; do not require CF Computer.

### Context / scoring

- Verifier remains Harbor TB2.1 (`/logs/verifier/reward.txt`).  
- Populate `AgentContext` from `tk exec --json` summary + log file (tokens, tool calls if present).  
- pass@1 = mean of binary rewards over tasks with `k=1`.

### Layout (proposed repo home)

```text
tschk/telekinesis
  bench/harbor/
    PLAN.md                 # this doc (symlink or copy)
    agents/tk_harbor_agent.py
    README.md               # run commands
    smoke_tasks.txt         # 3–5 task ids for Phase A/C
```

Alternate if we want harness-only: `tschk/rotary/bench/harbor/` — **prefer telekinesis** because the CLI under test is `tk exec` (host), not raw rx4.

### Example commands (once Docker exists)

```bash
pip install harbor
export OPENAI_API_KEY=...   # Codex + any OpenAI-backed model

# Phase A — Codex smoke
harbor run -d terminal-bench/terminal-bench-2-1 \
  --agent codex \
  -m openai/<FIXED_MODEL> \
  --task-id <id1> --task-id <id2> --task-id <id3> -k 1

# Phase C — custom tk
harbor run -d terminal-bench/terminal-bench-2-1 \
  --agent bench.harbor.agents.tk_harbor_agent:TkHarborAgent \
  -m openai/<FIXED_MODEL> \
  --ak model=openai/<FIXED_MODEL> \
  --task-id hello-world -k 1
```

(Exact `--agent` import path follows Harbor version; validate against `harbor agent list` / installed-agent API.)

## Model lock

Pick **one** model id for all three agents in a given table (e.g. `gpt-5` / whatever Max names). Document in results CSV:

`run_id, agent, model, task_id, reward, pass, seconds, cost_est`

## Out of scope

- Cloudflare Computer / orbs (tk-cloud)  
- Amp until Phase E  
- Changing rotary core for bench (adapter-only; host wiring only if `tk exec` gaps appear)

## Next actions

1. ~~Draft this plan~~  
2. Land `bench/harbor/` scaffold on telekinesis via omp (PR) — adapter stub + README  
3. Install Docker on box **or** run Phase A on cp.local  
4. Phase A smoke → report pass@1 %  
5. Phase C tk adapter smoke → compare

## Blockers

- **No Docker on box** (2026-09-13) — Phase A/C smoke pending  
- Fixed model id not yet named by Max — use placeholder until set  
