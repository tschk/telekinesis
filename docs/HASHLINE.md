# Hashline + prewalk

OMP-inspired accuracy slice in the host tool path. This is not a vendor of
[oh-my-pi](https://github.com/can1357/oh-my-pi). `tk` still uses rx4's loop;
only `read` / `write` / `edit` are replaced after `register_builtin_tools`.

## Why

Vanilla Pi / `apply_patch` / unique-string `edit` lose work on light models:
stale hunks land, unseen lines get invented, and no-ops look like success.
Hashline binds every edit to a snapshot tag from a real read.

## Edit protocol

`read` returns:

```text
[src/lib.rs#A1B2]
1:fn main() {}
```

`edit` takes `{ "input": "..." }`. File sections start with `[path#TAG]`.
Ops: `PUT N.=M:` + `+body` rows, `PUT <N:` / `PUT >N:` / `PUT >$:`,
`CUT N.=M`, `REM`, `MV DEST`. Numbers are the original snapshot.

Fail closed:

- stale `TAG` (store or live bytes)
- hunk on a line the latest read did not display (summarizing reads elide the middle)
- byte-identical no-op

Kimi / DeepSeek-class apply models get a sloppy parse (`PUT 2-2:`, bare body
rows). `apply_patch` / unified diff is not the default edit path.

Successful writes return a fresh `[path#TAG]` so the next hunk can re-ground.
`Diagnostics: unavailable` is appended to the last section in a batch; default
`tk` does not enable rx4 `ipc` LSP.

## Model roles

Roles: `default` / `smol` / `slow` / `plan`.

```bash
tk exec --model gpt-5.6-sol --smol gpt-5.6-sol-light --prewalk "fix the test"
tk exec --plan-yolo --plan-model gpt-5.6-sol --smol gpt-5.6-sol-light "implement the plan"
```

Env equivalents: `TK_SMOL_MODEL`, `TK_SLOW_MODEL`, `TK_PLAN_MODEL`,
`TK_PREWALK=1`, `TK_PLAN_YOLO=1`. Flags win over env.

`--prewalk` (and `--plan-yolo`) start on the investigate/plan model. The first
real `write` / `edit` switches one-way to `--smol` for later LLM turns in the
same run. `--no-yolo` and slim defaults are unchanged.

## Tests

`ui/tui` covers apply, stale-tag reject, no-op fail, unseen-line reject, and
prewalk switch. Run `cd ui/tui && cargo test`.
