#!/usr/bin/env bash

avo_die() {
  printf '%s\n' "$*" >&2
  exit 2
}

avo_fail() {
  printf '%s\n' "$*" >&2
  exit 1
}

avo_git_root() {
  git rev-parse --show-toplevel 2>/dev/null || avo_die "usage: run from a git repository (or pass --init <task>)"
}

avo_abs() {
  local path=$1
  case "$path" in
    /*) printf '%s\n' "$path" ;;
    *) printf '%s\n' "$PWD/$path" ;;
  esac
}

avo_task_dir() {
  printf '%s/.avo/%s\n' "$AVO_ROOT" "$AVO_TASK"
}

avo_config_path() {
  printf '%s/config.json\n' "$(avo_task_dir)"
}

avo_ledger_path() {
  printf '%s/ledger.jsonl\n' "$(avo_task_dir)"
}

avo_branch_name() {
  printf 'avo/%s\n' "$AVO_TASK"
}

avo_read_json_file() {
  local file=$1
  local expr=$2
  python3 - "$file" "$expr" <<'PY'
import json, sys
obj = json.load(open(sys.argv[1]))
print(obj.get(sys.argv[2], "") if sys.argv[2] != "*" else json.dumps(obj))
PY
}

avo_load_current() {
  if [ -n "${AVO_TASK:-}" ]; then
    return 0
  fi
  if [ -f "$AVO_ROOT/.avo/current" ]; then
    AVO_TASK=$(tr -d '[:space:]' <"$AVO_ROOT/.avo/current")
    return 0
  fi
  local branch
  branch=$(git -C "$AVO_ROOT" rev-parse --abbrev-ref HEAD)
  case "$branch" in
    avo/*) AVO_TASK=${branch#avo/} ;;
  esac
}

avo_load_config() {
  avo_load_current
  [ -n "${AVO_TASK:-}" ] || avo_die "usage: $0 --init <task> --goal <text> --score <cmd> [--agent <cmd>]"
  local cfg
  cfg=$(avo_config_path)
  [ -f "$cfg" ] || avo_die "usage: $0 --init <task> --goal <text> --score <cmd> [--agent <cmd>]"
  AVO_GOAL=${AVO_GOAL:-$(avo_read_json_file "$cfg" goal)}
  AVO_SCORE=${AVO_SCORE:-$(avo_read_json_file "$cfg" score)}
  AVO_AGENT=${AVO_AGENT:-$(avo_read_json_file "$cfg" agent)}
  AVO_BRANCH=$(avo_read_json_file "$cfg" branch)
  [ -n "$AVO_BRANCH" ] || AVO_BRANCH=$(avo_branch_name)
}

avo_write_config() {
  local dir cfg
  dir=$(avo_task_dir)
  mkdir -p "$dir"
  cfg=$(avo_config_path)
  python3 - "$cfg" "$AVO_TASK" "$AVO_GOAL" "$AVO_SCORE" "$AVO_AGENT" "$(avo_branch_name)" <<'PY'
import json, sys
path, task, goal, score, agent, branch = sys.argv[1:]
json.dump({
    "task": task,
    "goal": goal,
    "score": score,
    "agent": agent,
    "branch": branch,
}, open(path, "w"), indent=2)
open(path, "a").write("\n")
PY
  printf '%s\n' "$AVO_TASK" >"$AVO_ROOT/.avo/current"
  if [ ! -f "$dir/notes.md" ]; then
    printf '%s\n' "$AVO_GOAL" >"$dir/notes.md"
  fi
  if ! grep -qxF '.avo/' "$AVO_ROOT/.git/info/exclude" 2>/dev/null; then
    printf '%s\n' '.avo/' >>"$AVO_ROOT/.git/info/exclude"
  fi
}

avo_current_branch() {
  git -C "$AVO_ROOT" rev-parse --abbrev-ref HEAD
}

avo_refuse_main() {
  local branch
  branch=$(avo_current_branch)
  case "$branch" in
    main|master)
      avo_fail "refusing to run on $branch; use --init <task> to create avo/<task>"
      ;;
  esac
}

avo_require_branch() {
  local branch expected
  branch=$(avo_current_branch)
  expected=$(avo_branch_name)
  case "$branch" in
    main|master)
      avo_fail "refusing to run on $branch; checkout $expected"
      ;;
  esac
  if [ "$branch" != "$expected" ]; then
    avo_fail "HEAD is $branch; AVO work must stay on $expected"
  fi
}

avo_init() {
  [ -n "${AVO_TASK:-}" ] || avo_die "usage: $0 --init <task> --goal <text> --score <cmd> [--agent <cmd>]"
  [ -n "${AVO_GOAL:-}" ] || avo_die "usage: $0 --init <task> --goal <text> --score <cmd> [--agent <cmd>]"
  [ -n "${AVO_SCORE:-}" ] || avo_die "usage: $0 --init <task> --goal <text> --score <cmd> [--agent <cmd>]"
  AVO_AGENT=${AVO_AGENT:-$AVO_DEFAULT_AGENT}
  AVO_SCORE=$(avo_abs "$AVO_SCORE")
  AVO_AGENT=$(avo_abs "$AVO_AGENT")
  [ -x "$AVO_SCORE" ] || avo_die "score command is not executable: $AVO_SCORE"
  [ -x "$AVO_AGENT" ] || avo_die "agent command is not executable: $AVO_AGENT"
  local expected
  expected=$(avo_branch_name)
  if [ "$(avo_current_branch)" != "$expected" ]; then
    git -C "$AVO_ROOT" checkout -q -b "$expected"
  fi
  avo_write_config
}

avo_dirty() {
  if [ -n "$(git -C "$AVO_ROOT" status --porcelain --untracked-files=no)" ]; then
    return 0
  fi
  return 1
}

avo_lock_dir() {
  printf '%s/lock\n' "$(avo_task_dir)"
}

avo_acquire_lock() {
  local lock pid
  lock=$(avo_lock_dir)
  mkdir -p "$(avo_task_dir)"
  if [ -d "$lock" ]; then
    pid=$(tr -d '[:space:]' <"$lock/pid" 2>/dev/null || true)
    if [ -n "$pid" ] && { kill -0 "$pid" 2>/dev/null || [ -d "/proc/$pid" ]; }; then
      avo_fail "tick locked by pid $pid"
    fi
    rm -rf "$lock"
  fi
  mkdir "$lock" || avo_fail "tick locked"
  printf '%s\n' "$$" >"$lock/pid"
}

avo_release_lock() {
  rm -rf "$(avo_lock_dir)"
}

avo_restore_tree() {
  git -C "$AVO_ROOT" reset -q --hard HEAD
  git -C "$AVO_ROOT" clean -q -fd -e .avo -e .last-prompt
}

avo_commit_candidate() {
  local msg=$1
  (
    cd "$AVO_ROOT"
    git add -A
    git reset -q -- .avo .last-prompt 2>/dev/null || true
    git commit -q --allow-empty -m "$msg"
  )
}

avo_ledger_append() {
  local kind=$1
  local payload=$2
  python3 - "$(avo_ledger_path)" "$kind" "$payload" <<'PY'
import json, sys, time
path, kind, payload = sys.argv[1], sys.argv[2], sys.argv[3]
row = json.loads(payload)
row["kind"] = kind
row.setdefault("ts", time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()))
with open(path, "a") as fh:
    fh.write(json.dumps(row, separators=(",", ":")) + "\n")
PY
}

avo_ledger_rows() {
  python3 - "$(avo_ledger_path)" <<'PY'
import json, sys
path = sys.argv[1]
try:
    rows = [json.loads(line) for line in open(path) if line.strip()]
except FileNotFoundError:
    rows = []
print(json.dumps(rows))
PY
}

avo_best_objective() {
  python3 - "$(avo_ledger_path)" <<'PY'
import json, sys
path = sys.argv[1]
best = None
try:
    rows = [json.loads(line) for line in open(path) if line.strip()]
except FileNotFoundError:
    rows = []
for row in rows:
    if row.get("kind") != "accept":
        continue
    obj = float(row.get("objective", 0))
    if best is None or obj > best:
        best = obj
print("" if best is None else best)
PY
}

avo_tick_number() {
  python3 - "$(avo_ledger_path)" <<'PY'
import json, sys
path = sys.argv[1]
n = 0
try:
    for line in open(path):
        if not line.strip():
            continue
        row = json.loads(line)
        if row.get("kind") in ("accept", "reject", "error"):
            n += 1
except FileNotFoundError:
    pass
print(n + 1)
PY
}

avo_should_supervise() {
  local stall_after=${AVO_STALL_AFTER:-3}
  python3 - "$(avo_ledger_path)" "$stall_after" <<'PY'
import json, sys
path, stall_after = sys.argv[1], int(sys.argv[2])
try:
    rows = [json.loads(line) for line in open(path) if line.strip()]
except FileNotFoundError:
    rows = []
best = None
since_best = 0
since_supervisor = 0
for row in rows:
    kind = row.get("kind")
    if kind == "supervisor":
        since_supervisor = 0
        continue
    if kind not in ("accept", "reject", "error"):
        continue
    since_supervisor += 1
    obj = float(row.get("objective", 0))
    improved = kind == "accept" and (best is None or obj > best)
    if improved:
        best = obj
        since_best = 0
    else:
        since_best += 1
print("yes" if since_best >= stall_after and since_supervisor >= stall_after else "no")
PY
}

avo_parse_score() {
  local raw=$1
  python3 - "$raw" <<'PY'
import json, sys
raw = sys.argv[1]
obj = json.loads(raw)
if "correct" not in obj or "objective" not in obj or "note" not in obj:
    raise SystemExit("score JSON must include correct, objective, note")
correct = bool(obj["correct"])
objective = 0.0 if not correct else float(obj["objective"])
if not correct:
    objective = 0.0
metrics = obj.get("metrics") or {}
stddev = float(metrics.get("stddev", metrics.get("sigma", 0)) or 0)
print(json.dumps({
    "correct": correct,
    "objective": objective,
    "note": str(obj["note"]),
    "stddev": stddev,
    "raw": obj,
}))
PY
}

avo_should_commit() {
  local correct=$1
  local objective=$2
  local stddev=$3
  local best=$4
  python3 - "$correct" "$objective" "$stddev" "$best" "${AVO_MIN_IMPROVEMENT:-0}" "${AVO_NOISE_K:-1}" <<'PY'
import sys
correct, objective, stddev, best, min_imp, noise_k = sys.argv[1:7]
if correct != "true":
    print("no")
    raise SystemExit
objective = float(objective)
stddev = float(stddev or 0)
min_imp = float(min_imp or 0)
noise_k = float(noise_k or 1)
margin = noise_k * stddev
if best == "":
    print("yes")
    raise SystemExit
print("yes" if objective + margin >= float(best) + min_imp else "no")
PY
}

avo_lineage_text() {
  local n=${AVO_LINEAGE_N:-8}
  python3 - "$(avo_ledger_path)" "$n" <<'PY'
import json, sys
path, n = sys.argv[1], int(sys.argv[2])
try:
    rows = [json.loads(line) for line in open(path) if line.strip()]
except FileNotFoundError:
    rows = []
rows = [row for row in rows if row.get("kind") in ("accept", "reject")]
rows = rows[-n:]
if not rows:
    print("(empty — this is the first variation)")
    raise SystemExit
for row in rows:
    print(
        f"tick {row.get('tick')} kind={row.get('kind')} correct={row.get('correct')} "
        f"objective={row.get('objective')} note={row.get('note')}"
    )
PY
}

avo_knowledge_list() {
  local dir
  dir=$(avo_task_dir)
  printf '%s\n' "docs/AVO.md"
  printf '%s\n' "${dir#"$AVO_ROOT"/}/notes.md"
  if [ -f "$dir/supervisor.md" ]; then
    printf '%s\n' "${dir#"$AVO_ROOT"/}/supervisor.md"
  fi
}

avo_build_driver_prompt() {
  local out=$1
  local best
  best=$(avo_best_objective)
  {
    printf '%s\n' "You are the AVO variation operator, not a one-shot generator."
    printf '%s\n' "Vary(P_t, K, f) = Agent(P_t, K, f)."
    printf '%s\n' "Consult the scored lineage P_t, knowledge files K, and scoring function f."
    printf '%s\n' "Edit the working tree in place. You may run f, diagnose, and revise before exiting."
    printf '%s\n' "Do not commit. Do not push. Stay on branch $(avo_branch_name)."
    printf '\n'
    printf '%s\n' "Goal:"
    printf '%s\n' "$AVO_GOAL"
    printf '\n'
    printf '%s\n' "Knowledge K (read these files):"
    avo_knowledge_list
    printf '\n'
    printf '%s\n' "Scoring function f:"
    printf '%s\n' "  $AVO_SCORE $AVO_ROOT"
    printf '%s\n' "  JSON {correct, objective, metrics, note, artifacts}"
    printf '%s\n' "  Incorrect candidates score objective 0. Host commits only if correct and objective >= best so far (within noise margin)."
    if [ -n "$best" ]; then
      printf '%s\n' "  Best committed objective so far: $best"
    else
      printf '%s\n' "  No committed best yet."
    fi
    printf '\n'
    printf '%s\n' "Lineage P_t (recent scored candidates):"
    avo_lineage_text
    if [ -f "$(avo_task_dir)/supervisor.md" ]; then
      printf '\n'
      printf '%s\n' "Supervisor directions:"
      cat "$(avo_task_dir)/supervisor.md"
    fi
  } >"$out"
}

avo_build_supervisor_prompt() {
  local out=$1
  {
    printf '%s\n' "You are the AVO supervisor, not the variation operator."
    printf '%s\n' "The search stalled. Review the evolutionary trajectory and propose new directions."
    printf '%s\n' "Write concrete next directions. Do not implement. Do not commit. Do not push."
    printf '\n'
    printf '%s\n' "Goal:"
    printf '%s\n' "$AVO_GOAL"
    printf '\n'
    printf '%s\n' "Full lineage:"
    avo_lineage_text
    printf '\n'
    printf '%s\n' "$(avo_ledger_rows)"
  } >"$out"
}

avo_run_score() {
  local raw
  raw=$("$AVO_SCORE" "$AVO_ROOT") || return 3
  avo_parse_score "$raw"
}

avo_run_supervisor() {
  local dir prompt
  dir=$(avo_task_dir)
  prompt="$dir/supervisor.prompt"
  avo_build_supervisor_prompt "$prompt"
  {
    printf '%s\n' "Stall detected: no new best for ${AVO_STALL_AFTER:-3} ticks."
    printf '%s\n' "Propose new directions from the trajectory."
    printf '\n'
    if [ -n "${AVO_SUPERVISOR_MODEL:-}" ]; then
      AVO_MODEL="$AVO_SUPERVISOR_MODEL" "$AVO_AGENT" "$AVO_ROOT" "$prompt" || true
    else
      printf '%s\n' "No AVO_SUPERVISOR_MODEL set; host-only stall note."
    fi
  } >"$dir/supervisor.md"
  avo_restore_tree
  avo_ledger_append supervisor "$(python3 - "$AVO_TASK" "${AVO_SUPERVISOR_MODEL:-}" <<'PY'
import json, sys
print(json.dumps({"task": sys.argv[1], "model": sys.argv[2], "note": "supervisor redirect"}))
PY
)"
}
