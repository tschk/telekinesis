# AVO on telekinesis

Source of truth: NVIDIA, *AVO: Agentic Variation Operators*
([arXiv:2603.24517](https://arxiv.org/abs/2603.24517)). The loop lives in
`scripts/avo/`. Do not vendor [avo-lite](https://github.com/Git-on-my-level/avo-lite).
There is no `tk avo` subcommand; the default binary stays slim.

## NVIDIA loop

Prior LLM-in-the-loop search is Sample-then-Generate:

```
Vary(P_t) = Generate(Sample(P_t))
```

AVO replaces that pipeline. The agent *is* the variation operator:

```
Vary(P_t, K, f) = Agent(P_t, K, f)
```

- `P_t` is a scored lineage of `(candidate, f(candidate))`. Every tick feeds
  recent lineage + notes back as context.
- `K` is files the agent can read (`docs/AVO.md` + `.avo/<task>/notes.md`).
- `f` is two-part: hard correctness first. Incorrect candidates score `0`
  regardless of the objective. The host commits only when `correct` and
  `objective >= best` (or within the noise margin).
- Committed versions persist on `avo/<task>`. Never `main`. Never push.
- A no-LLM stall detector watches the ledger. On plateau, a supervisor pass
  reviews the trajectory and proposes directions via
  `tk exec --model "$AVO_SUPERVISOR_MODEL"`.

`tk exec` is already an agent loop with tools, not a one-shot completion.
That is the operator.

## Wire it

```bash
scripts/avo/tick.sh --init speedup \
  --goal  "maximize a higher-is-better objective" \
  --score "$PWD/scripts/adapters/score-example.sh" \
  --agent "$PWD/scripts/adapters/agent-tk.sh"

scripts/avo/tick.sh
scripts/avo/run.sh --max-ticks 8
scripts/avo/status.sh
```

`tick.sh` is one locked idempotent variation step. `run.sh` bounds ticks.
`status.sh` prints best score, stall streak, and recent lineage.

`scripts/adapters/agent-tk.sh <candidate-dir> <prompt-file>` runs
`tk exec --cwd <dir> [- --model <AVO_MODEL>] -` with the prompt on stdin.
Default non-TTY yolo (`AlwaysAllow`) stays as-is; the adapter never passes
`--no-yolo`.

`AVO_MODEL` wins, then `AVO_DRIVER_MODEL` / `AVO_SUPERVISOR_MODEL` (cheap
driver, stronger supervisor). `TK` overrides the binary name.

## Scorer `f`

Replace or configure `score-example.sh`. It must print one JSON object and
exit 0 when evaluation *completed* (pass or fail); non-zero is infra failure:

```json
{"correct": true, "objective": 0.87, "metrics": {"elapsed_s": 0.4}, "note": "what changed", "artifacts": []}
```

`correct` is the hard gate and must be a JSON boolean (`true`/`false`).
A string such as `"false"` is rejected as infra failure (fail closed).
`objective` is higher-is-better. `note` is what the next agent reads.
`score-example.sh` runs `AVO_TEST_CMD` as the correctness gate; on success
the default objective is `1/elapsed_s`. Failed tests force `objective` to 0.

The host scores the starting tree as a baseline before the first accept, so
the first candidate is ratcheted. Rejected candidates are persisted as an
immutable `refs/avo/<task>/reject/<tick>` commit plus a patch under
`.avo/<task>/rejected/` and fed back in `P_t`. Restore returns to the
pre-tick branch/HEAD and only drops that tick's edits; it never
`git clean -fd`s the user's pre-existing untracked files or allowed dirty
work.

Accept rule: `objective + k*stddev >= best + min_improvement`.
Defaults: `AVO_MIN_IMPROVEMENT=0`, `AVO_NOISE_K=1`, `stddev` from
`metrics.stddev`. A match (including within noise) is committed; a regression
beyond the margin is restored.

Optional: `AVO_CORRECT_CMD`, `AVO_OBJECTIVE`, `AVO_NOTE`, `AVO_STALL_AFTER`
(default 3).

`scripts/check-avo-adapters.sh` and `scripts/check-avo-loop.sh` cover the
adapter contract and the host loop (lineage, ratchet, stall→supervisor, lock,
never push).

rx4 stays `default-features = false` with providers + builtin-tools only.

## Engine helpers
