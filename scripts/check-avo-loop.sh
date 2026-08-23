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
set +e
kill "$lock_pid" 2>/dev/null
wait "$lock_pid" 2>/dev/null
set -e
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

# baseline ratchet: a worse first candidate must not commit
repo_base="$workdir/repo_base"
git_init "$repo_base"
write_agent "$workdir/agent_base.sh"
write_scorer "$workdir/score_base.sh"
(cd "$repo_base" && "$tick" --init ratchet --goal "g" --score "$workdir/score_base.sh" --agent "$workdir/agent_base.sh") >/tmp/avo-base-init 2>&1
printf '10\n' >"$repo_base/value.txt"
git -C "$repo_base" add value.txt
git -C "$repo_base" commit -q -m 'strong baseline'
(cd "$repo_base" && FAKE_VALUE=1 "$tick") >/tmp/avo-base-worse 2>&1
test "$(tr -d '[:space:]' <"$repo_base/value.txt")" = "10" || fail "first candidate below baseline must restore"
python3 - "$repo_base/.avo/ratchet/ledger.jsonl" <<'PY'
import json, sys
rows = [json.loads(line) for line in open(sys.argv[1]) if line.strip()]
kinds = [row.get("kind") for row in rows]
assert "baseline" in kinds, kinds
assert any(row.get("kind") == "reject" for row in rows), kinds
assert not any(row.get("kind") == "accept" for row in rows), kinds
PY

# persist rejected candidate as immutable ref + patch
python3 - "$repo/.avo/demo/ledger.jsonl" <<'PY'
import json, sys
rows = [json.loads(line) for line in open(sys.argv[1]) if line.strip()]
rej = [row for row in rows if row.get("kind") == "reject"]
assert rej, rows
assert any(row.get("candidate") for row in rej), rej
assert any(row.get("patch") for row in rej), rej
PY
test -f "$repo/.avo/demo/rejected/tick-3.patch" || test -n "$(ls "$repo/.avo/demo/rejected" 2>/dev/null)" || fail "rejected patch must be persisted"
git -C "$repo" show-ref | grep -q 'refs/avo/demo/reject/' || fail "rejected candidate ref must be persisted"

# string "false" must fail closed
cat >"$workdir/score_strfalse.sh" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' '{"correct":"false","objective":9,"note":"string false"}'
EOF
chmod +x "$workdir/score_strfalse.sh"
repo_sf="$workdir/repo_sf"
git_init "$repo_sf"
write_agent "$workdir/agent_sf.sh"
(cd "$repo_sf" && "$tick" --init strfalse --goal "g" --score "$workdir/score_strfalse.sh" --agent "$workdir/agent_sf.sh") >/tmp/avo-sf-init 2>&1
set +e
(cd "$repo_sf" && FAKE_VALUE=4 "$tick") >/tmp/avo-sf 2>&1
sf_status=$?
set -e
test "$sf_status" -ne 0 || fail "string correct=false must fail closed"
test "$(cat "$repo_sf/value.txt")" = "seed" || fail "string-false score must restore the tree"
if grep -q '"kind":"accept"' "$repo_sf/.avo/strfalse/ledger.jsonl" 2>/dev/null; then
  fail "string-false must not accept"
fi

# preserve user untracked and allowed dirty on reject
repo_keep="$workdir/repo_keep"
git_init "$repo_keep"
write_agent "$workdir/agent_keep.sh"
write_scorer "$workdir/score_keep.sh"
(cd "$repo_keep" && "$tick" --init keep --goal "g" --score "$workdir/score_keep.sh" --agent "$workdir/agent_keep.sh") >/tmp/avo-keep-init 2>&1
printf 'mine\n' >"$repo_keep/user-untracked.txt"
printf 'dirty-notes\n' >>"$repo_keep/notes.md"
(cd "$repo_keep" && FAKE_VALUE=bad "$tick" --allow-dirty) >/tmp/avo-keep 2>&1
test -f "$repo_keep/user-untracked.txt" || fail "restore must keep user untracked"
grep -q 'dirty-notes' "$repo_keep/notes.md" || fail "restore must keep allowed dirty"
test "$(cat "$repo_keep/value.txt")" = "seed" || fail "reject must restore value.txt"

# scorer artifacts must not enter the accepted commit
cat >"$workdir/score_side.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
dir=${1:?}
printf 'side-effect\n' >"$dir/scorer-artifact.txt"
printf 'mutated\n' >"$dir/notes.md"
value=$(tr -d '[:space:]' <"$dir/value.txt")
python3 - "$value" <<'PY'
import json, sys
raw=sys.argv[1]
try:
    obj=float(raw); correct=True; note="ok"
except ValueError:
    obj=0.0; correct=False; note="incorrect candidate"
print(json.dumps({"correct": correct, "objective": obj, "metrics": {}, "note": note, "artifacts": []}))
PY
EOF
chmod +x "$workdir/score_side.sh"
repo_side="$workdir/repo_side"
git_init "$repo_side"
write_agent "$workdir/agent_side.sh"
(cd "$repo_side" && "$tick" --init side --goal "g" --score "$workdir/score_side.sh" --agent "$workdir/agent_side.sh") >/tmp/avo-side-init 2>&1
(cd "$repo_side" && FAKE_VALUE=6 "$tick") >/tmp/avo-side 2>&1
if git -C "$repo_side" ls-tree -r --name-only HEAD | grep -q scorer-artifact; then
  fail "scorer artifact must not be in the candidate commit"
fi
if git -C "$repo_side" show HEAD:notes.md | grep -q mutated; then
  fail "scorer tracked mutation must not be in the candidate commit"
fi
test "$(git -C "$repo_side" show HEAD:value.txt | tr -d '[:space:]')" = "6" || fail "agent diff must be in the candidate commit"

# agent that commits must be unwound to start HEAD before accept
cat >"$workdir/agent_commit.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
dir=$1
printf '7\n' >"$dir/value.txt"
git -C "$dir" add value.txt
git -C "$dir" commit -q -m 'agent commit'
EOF
chmod +x "$workdir/agent_commit.sh"
repo_ac="$workdir/repo_ac"
git_init "$repo_ac"
write_scorer "$workdir/score_ac.sh"
(cd "$repo_ac" && "$tick" --init agentc --goal "g" --score "$workdir/score_ac.sh" --agent "$workdir/agent_commit.sh") >/tmp/avo-ac-init 2>&1
start_msg=$(git -C "$repo_ac" log -1 --format=%s)
(cd "$repo_ac" && "$tick") >/tmp/avo-ac 2>&1
test "$(git -C "$repo_ac" log -1 --format=%s)" != "agent commit" || fail "host must not leave the agent commit as HEAD"
test "$(tr -d '[:space:]' <"$repo_ac/value.txt")" = "7" || fail "unwound agent commit should still accept the tree"

# agent that switches branch must be rejected and start branch restored
cat >"$workdir/agent_switch.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
dir=$1
printf '8\n' >"$dir/value.txt"
git -C "$dir" checkout -q main
EOF
chmod +x "$workdir/agent_switch.sh"
repo_sw="$workdir/repo_sw"
git_init "$repo_sw"
write_scorer "$workdir/score_sw.sh"
(cd "$repo_sw" && "$tick" --init switch --goal "g" --score "$workdir/score_sw.sh" --agent "$workdir/agent_switch.sh") >/tmp/avo-sw-init 2>&1
main_before=$(git -C "$repo_sw" rev-parse main)
set +e
(cd "$repo_sw" && "$tick") >/tmp/avo-sw 2>&1
sw_status=$?
set -e
test "$sw_status" -ne 0 || fail "branch switch must fail closed"
test "$(git -C "$repo_sw" rev-parse --abbrev-ref HEAD)" = "avo/switch" || fail "must restore start branch"
test "$(git -C "$repo_sw" rev-parse main)" = "$main_before" || fail "must not rewrite main"

# infer task from avo/* branch before .avo/current
repo_br="$workdir/repo_br"
git_init "$repo_br"
write_agent "$workdir/agent_br.sh"
write_scorer "$workdir/score_br.sh"
(cd "$repo_br" && "$tick" --init task-a --goal "ga" --score "$workdir/score_br.sh" --agent "$workdir/agent_br.sh") >/tmp/avo-bra 2>&1
git -C "$repo_br" checkout -q main
(cd "$repo_br" && "$tick" --init task-b --goal "gb" --score "$workdir/score_br.sh" --agent "$workdir/agent_br.sh") >/tmp/avo-brb 2>&1
test "$(tr -d '[:space:]' <"$repo_br/.avo/current")" = "task-b"
git -C "$repo_br" checkout -q avo/task-a
(cd "$repo_br" && FAKE_VALUE=2 "$tick") >/tmp/avo-bra-tick 2>&1
test -f "$repo_br/.avo/task-a/ledger.jsonl" || fail "tick must use avo/task-a"
python3 - "$repo_br/.avo/task-a/ledger.jsonl" <<'PY'
import json, sys
rows = [json.loads(line) for line in open(sys.argv[1]) if line.strip()]
assert any(row.get("kind") in ("accept", "reject", "baseline") for row in rows), rows
PY

# worktree exclude path
repo_wt="$workdir/repo_wt"
git_init "$repo_wt"
wt="$workdir/wt"
git -C "$repo_wt" worktree add -q "$wt"
write_agent "$workdir/agent_wt.sh"
write_scorer "$workdir/score_wt.sh"
(cd "$wt" && "$tick" --init wtree --goal "g" --score "$workdir/score_wt.sh" --agent "$workdir/agent_wt.sh") >/tmp/avo-wt 2>&1
test -f "$wt/.avo/wtree/config.json" || fail "init from worktree must write config"
git -C "$wt" rev-parse --git-path info/exclude >/tmp/avo-wt-exclude
test -s /tmp/avo-wt-exclude || fail "worktree exclude path must resolve"

# run.sh --goal must not add an extra tick
repo_run="$workdir/repo_run"
git_init "$repo_run"
write_agent "$workdir/agent_run.sh"
write_scorer "$workdir/score_run.sh"
(cd "$repo_run" && "$tick" --init rungoal --goal "g" --score "$workdir/score_run.sh" --agent "$workdir/agent_run.sh") >/tmp/avo-rg-init 2>&1
(cd "$repo_run" && "$run" --goal "g2" --max-ticks 2) >/tmp/avo-rg 2>&1
python3 - "$repo_run/.avo/rungoal/ledger.jsonl" <<'PY'
import json, sys
rows = [json.loads(line) for line in open(sys.argv[1]) if line.strip()]
scored = [row for row in rows if row.get("kind") in ("accept", "reject", "error")]
assert len(scored) == 2, rows
PY

# supervisor failure must not record a success row
cat >"$workdir/agent_supfail.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
dir=$1
if [ -n "${AVO_MODEL:-}" ]; then
  exit 7
fi
if [ -n "${FAKE_VALUE:-}" ]; then
  printf '%s\n' "$FAKE_VALUE" >"$dir/value.txt"
fi
EOF
chmod +x "$workdir/agent_supfail.sh"
repo_sup="$workdir/repo_sup"
git_init "$repo_sup"
write_scorer "$workdir/score_sup.sh"
(cd "$repo_sup" && "$tick" --init supfail --goal "g" --score "$workdir/score_sup.sh" --agent "$workdir/agent_supfail.sh") >/tmp/avo-sup-init 2>&1
export AVO_STALL_AFTER=1
export AVO_SUPERVISOR_MODEL=supervisor-strong
(cd "$repo_sup" && FAKE_VALUE=bad "$tick") >/tmp/avo-sup1 2>&1
python3 - "$repo_sup/.avo/supfail/ledger.jsonl" <<'PY'
import json, sys
rows = [json.loads(line) for line in open(sys.argv[1]) if line.strip()]
kinds = [row.get("kind") for row in rows]
assert "supervisor_error" in kinds, kinds
assert "supervisor" not in kinds, kinds
PY
unset AVO_STALL_AFTER AVO_SUPERVISOR_MODEL

# stale lock reclaim is atomic (mkdir after rename, no rm of a live lock path)
repo_stale="$workdir/repo_stale"
git_init "$repo_stale"
write_agent "$workdir/agent_stale.sh"
write_scorer "$workdir/score_stale.sh"
(cd "$repo_stale" && "$tick" --init stale --goal "g" --score "$workdir/score_stale.sh" --agent "$workdir/agent_stale.sh") >/tmp/avo-stale-init 2>&1
mkdir -p "$repo_stale/.avo/stale/lock"
printf '1\n' >"$repo_stale/.avo/stale/lock/pid"
set +e
(cd "$repo_stale" && FAKE_VALUE=3 "$tick") >/tmp/avo-stale 2>&1
stale_status=$?
set -e
test "$stale_status" -eq 0 || fail "unsignalable lock pid must be reclaimed"
test ! -d "$repo_stale/.avo/stale/lock" || fail "successful tick must release lock"
python3 - "$repo_stale/.avo/stale/ledger.jsonl" <<'PY'
import json, sys
rows = [json.loads(line) for line in open(sys.argv[1]) if line.strip()]
assert any(row.get("kind") in ("accept", "reject") for row in rows), rows
PY

echo "avo loop checks passed"
