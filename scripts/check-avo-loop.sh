#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
tick="$root/scripts/avo/tick.sh"
run="$root/scripts/avo/run.sh"
status="$root/scripts/avo/status.sh"
agent_tk="$root/scripts/adapters/agent-tk.sh"
score_ex="$root/scripts/adapters/score-example.sh"

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  exit 1
}

test -x "$tick" || fail "tick.sh must be executable"
test -x "$run" || fail "run.sh must be executable"
test -x "$status" || fail "status.sh must be executable"

if grep -nE 'git[[:space:]]+push' "$root/scripts/avo"/* >/tmp/avo-push-hits 2>/dev/null; then
  cat /tmp/avo-push-hits >&2
  fail "scripts/avo must never invoke git push"
fi

if grep -nE 'avo-lite' "$root/scripts/avo"/* >/tmp/avo-lite-hits 2>/dev/null; then
  cat /tmp/avo-lite-hits >&2
  fail "do not vendor or call avo-lite from scripts/avo"
fi

workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT

git_init() {
  local dir=$1
  mkdir -p "$dir"
  git -C "$dir" init -q -b main
  git -C "$dir" config user.email avo@test
  git -C "$dir" config user.name avo
  printf 'seed\n' >"$dir/value.txt"
  printf 'K notes\n' >"$dir/notes.md"
  mkdir -p "$dir/docs"
  printf '# knowledge\n' >"$dir/docs/AVO.md"
  git -C "$dir" add value.txt notes.md docs/AVO.md
  git -C "$dir" commit -q -m seed
}

write_agent() {
  local path=$1
  cat >"$path" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
dir=$1
prompt=$2
cp "$prompt" "$dir/.last-prompt"
if [ -n "${FAKE_VALUE:-}" ]; then
  printf '%s\n' "$FAKE_VALUE" >"$dir/value.txt"
fi
if [ -n "${FAKE_EXTRA:-}" ]; then
  printf '%s\n' "$FAKE_EXTRA" >"$dir/extra.txt"
fi
printf 'agent ran\n'
EOF
  chmod +x "$path"
}

write_scorer() {
  local path=$1
  cat >"$path" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
dir=${1:?}
value=$(tr -d '[:space:]' <"$dir/value.txt")
correct=true
note="value=$value"
case "$value" in
  bad|fail|'') correct=false; note="incorrect candidate" ;;
esac
if ! python3 -c 'import sys; float(sys.argv[1])' "$value" >/dev/null 2>&1; then
  correct=false
  note="incorrect candidate"
fi
if [ "$correct" = true ]; then
  objective=$value
else
  objective=0
fi
stddev="${AVO_STDDEV:-0}"
export SCORE_CORRECT="$correct"
export SCORE_OBJECTIVE="$objective"
export SCORE_NOTE="$note"
export SCORE_STDDEV="$stddev"
python3 - <<'PY'
import json, os
print(json.dumps({
    "correct": os.environ["SCORE_CORRECT"] == "true",
    "objective": float(os.environ["SCORE_OBJECTIVE"]),
    "metrics": {"stddev": float(os.environ["SCORE_STDDEV"])},
    "note": os.environ["SCORE_NOTE"],
    "artifacts": [],
}))
PY
EOF
  chmod +x "$path"
}

repo="$workdir/repo"
git_init "$repo"
agent="$workdir/agent.sh"
score="$workdir/score.sh"
write_agent "$agent"
write_scorer "$score"

set +e
"$tick" >/tmp/avo-tick-usage 2>&1
tick_status=$?
set -e
test "$tick_status" -eq 2 || fail "tick without args should exit 2, got $tick_status"
grep -q usage /tmp/avo-tick-usage || fail "tick usage missing"

set +e
(cd "$repo" && "$tick" --goal "maximize value" --score "$score" --agent "$agent") >/tmp/avo-main 2>&1
main_status=$?
set -e
test "$main_status" -ne 0 || fail "tick must refuse to run on main"
grep -Eiq 'main|avo/' /tmp/avo-main || fail "refuse-on-main message missing"

(cd "$repo" && "$tick" --init demo --goal "maximize value.txt (higher is better)" --score "$score" --agent "$agent") >/tmp/avo-init 2>&1
branch=$(git -C "$repo" rev-parse --abbrev-ref HEAD)
test "$branch" = "avo/demo" || fail "init must create avo/demo, got $branch"
test "$(git -C "$repo" branch --show-current)" != "main" || fail "must not stay on main"

(cd "$repo" && FAKE_VALUE=1 "$tick") >/tmp/avo-tick1 2>&1
commits=$(git -C "$repo" rev-list --count HEAD)
test "$commits" -eq 2 || fail "first correct improve should commit, commits=$commits"
test "$(cat "$repo/value.txt")" = "1"
grep -q 'Vary(P_t' "$repo/.last-prompt" || fail "prompt must state Vary(P_t, K, f) = Agent"
grep -qi 'variation operator' "$repo/.last-prompt" || fail "prompt must say the agent is the variation operator"
grep -q 'docs/AVO.md' "$repo/.last-prompt" || fail "prompt must point at knowledge K files"
grep -q 'maximize value.txt' "$repo/.last-prompt" || fail "prompt must include the goal"
grep -q "$score" "$repo/.last-prompt" || fail "prompt must include f / scorer path"
if grep -qi 'sample then' "$repo/.last-prompt"; then
  fail "prompt must not describe Sample-then-Generate"
fi

out=$(cd "$repo" && "$status")
printf '%s\n' "$out" | grep -q 'demo' || fail "status must show task"
printf '%s\n' "$out" | grep -q '1' || fail "status must show best/objective"

(cd "$repo" && FAKE_VALUE=3 "$tick") >/tmp/avo-tick2 2>&1
test "$(cat "$repo/value.txt")" = "3" || fail "better correct candidate must commit"
grep -q 'value=1' "$repo/.last-prompt" || fail "P_t lineage notes must be fed back"
grep -q 'objective' "$repo/.last-prompt" || fail "P_t scores must be fed back"

old_head=$(git -C "$repo" rev-parse HEAD)
(cd "$repo" && FAKE_VALUE=2 "$tick") >/tmp/avo-worse 2>&1
test "$(git -C "$repo" rev-parse HEAD)" = "$old_head" || fail "worse objective must not commit"
test "$(cat "$repo/value.txt")" = "3" || fail "reject must restore the committed tree"

old_head=$(git -C "$repo" rev-parse HEAD)
(cd "$repo" && FAKE_VALUE=bad "$tick") >/tmp/avo-bad 2>&1
test "$(git -C "$repo" rev-parse HEAD)" = "$old_head" || fail "incorrect candidate must not commit"
test "$(cat "$repo/value.txt")" = "3" || fail "incorrect reject must restore"
ledger="$repo/.avo/demo/ledger.jsonl"
python3 - "$ledger" <<'PY'
import json, sys
rows = [json.loads(line) for line in open(sys.argv[1]) if line.strip()]
bad = [row for row in rows if row.get("note") == "incorrect candidate"]
assert bad, rows
assert bad[-1]["correct"] is False
assert float(bad[-1]["objective"]) == 0
assert bad[-1]["kind"] == "reject"
PY

old_head=$(git -C "$repo" rev-parse HEAD)
(cd "$repo" && FAKE_VALUE=3 AVO_STDDEV=0.2 "$tick") >/tmp/avo-noise 2>&1
test "$(git -C "$repo" rev-parse HEAD)" != "$old_head" || fail "equal/within noise margin should commit"

repo2="$workdir/repo2"
git_init "$repo2"
write_agent "$workdir/agent2.sh"
write_scorer "$workdir/score2.sh"
(cd "$repo2" && "$tick" --init stall --goal "g" --score "$workdir/score2.sh" --agent "$workdir/agent2.sh") >/tmp/avo-stall-init 2>&1
export AVO_STALL_AFTER=2
export AVO_SUPERVISOR_MODEL=supervisor-strong
(cd "$repo2" && FAKE_VALUE=bad "$tick") >/tmp/avo-s1 2>&1
(cd "$repo2" && FAKE_VALUE=bad "$tick") >/tmp/avo-s2 2>&1
test -f "$repo2/.avo/stall/supervisor.md" || fail "stall detector must run a supervisor pass"
grep -qi 'direction' "$repo2/.avo/stall/supervisor.md" || true
grep -q 'supervisor-strong' "$repo2/.last-prompt" || grep -q 'supervisor' "$repo2/.avo/stall/ledger.jsonl" || fail "supervisor must be recorded"
python3 - "$repo2/.avo/stall/ledger.jsonl" <<'PY'
import json, sys
rows = [json.loads(line) for line in open(sys.argv[1]) if line.strip()]
kinds = [row.get("kind") for row in rows]
assert "supervisor" in kinds, kinds
PY
unset AVO_STALL_AFTER AVO_SUPERVISOR_MODEL

repo3="$workdir/repo3"
git_init "$repo3"
write_agent "$workdir/agent3.sh"
write_scorer "$workdir/score3.sh"
(cd "$repo3" && "$run" --init bounded --goal "g" --score "$workdir/score3.sh" --agent "$workdir/agent3.sh" --max-ticks 3) >/tmp/avo-run 2>&1
python3 - "$repo3/.avo/bounded/ledger.jsonl" <<'PY'
import json, sys
rows = [json.loads(line) for line in open(sys.argv[1]) if line.strip()]
scored = [row for row in rows if row.get("kind") in ("accept", "reject", "error")]
assert len(scored) == 3, rows
PY

lockrepo="$workdir/lockrepo"
git_init "$lockrepo"
(cd "$lockrepo" && "$tick" --init locked --goal "g" --score "$score" --agent "$agent") >/tmp/avo-lock-init 2>&1
mkdir -p "$lockrepo/.avo/locked/lock"
sleep 120 &
lock_pid=$!
printf '%s\n' "$lock_pid" >"$lockrepo/.avo/locked/lock/pid"
set +e
(cd "$lockrepo" && "$tick") >/tmp/avo-locked 2>&1
lock_status=$?
set -e
kill "$lock_pid" 2>/dev/null || true
test "$lock_status" -ne 0 || fail "tick must be locked / refuse when lock is held by a live pid"
rm -rf "$lockrepo/.avo/locked/lock"

printf '#!/bin/sh\nexit 0\n' >"$workdir/pass-test.sh"
printf '#!/bin/sh\nexit 1\n' >"$workdir/fail-test.sh"
chmod +x "$workdir/pass-test.sh" "$workdir/fail-test.sh"
pass_json=$(AVO_TEST_CMD="$workdir/pass-test.sh" "$score_ex" "$repo")
printf '%s\n' "$pass_json" | python3 -c '
import json, sys
obj = json.load(sys.stdin)
assert obj["correct"] is True, obj
assert float(obj["objective"]) > 0, obj
assert "elapsed_s" in obj["metrics"], obj
'
fail_json=$(AVO_TEST_CMD="$workdir/fail-test.sh" "$score_ex" "$repo")
printf '%s\n' "$fail_json" | python3 -c '
import json, sys
obj = json.load(sys.stdin)
assert obj["correct"] is False, obj
assert obj["objective"] == 0, obj
assert obj["note"] == "tests failed", obj
'

printf 'improve\n' >"$workdir/prompt.txt"
cat >"$workdir/tk" <<'EOF'
#!/usr/bin/env bash
printf 'argv:%s\n' "$*"
printf 'prompt:'
cat
EOF
chmod +x "$workdir/tk"
out=$(cd "$workdir" && TK="$workdir/tk" AVO_MODEL=supervisor-strong AVO_DRIVER_MODEL=driver-cheap "$agent_tk" . prompt.txt)
printf '%s\n' "$out" | grep -F -- '--model supervisor-strong' >/dev/null || fail "AVO_MODEL must win for supervisor tk exec --model"
if printf '%s\n' "$out" | grep -F -- '--no-yolo' >/dev/null; then
  fail "agent-tk must not pass --no-yolo"
fi

echo "avo loop checks passed"
