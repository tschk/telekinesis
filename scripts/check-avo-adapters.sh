#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
agent="$root/scripts/adapters/agent-tk.sh"
score="$root/scripts/adapters/score-example.sh"
lua=${AVO_LUA:-luajit}
assertions="$root/scripts/avo/assert.lua"

test -x "$agent"
test -x "$score"

set +e
"$agent" >/tmp/agent-tk-usage 2>&1
agent_status=$?
set -e
test "$agent_status" -eq 2
grep -q usage /tmp/agent-tk-usage

"$score" "$root" | "$lua" "$assertions" score valid

AVO_CORRECT_CMD="false" AVO_NOTE="unused" "$score" "$root" | "$lua" "$assertions" score incorrect

workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT
printf 'improve the candidate\n' >"$workdir/prompt.txt"
cat >"$workdir/tk" <<'EOF'
#!/usr/bin/env bash
printf 'argv:%s\n' "$*"
printf 'prompt:'
cat
EOF
chmod +x "$workdir/tk"
out=$(cd "$workdir" && TK="$workdir/tk" AVO_DRIVER_MODEL=grok-4.5 "$agent" . prompt.txt)
printf '%s\n' "$out" | grep -F 'argv:exec --cwd' >/dev/null
printf '%s\n' "$out" | grep -F -- '--model grok-4.5' >/dev/null
printf '%s\n' "$out" | grep -F 'prompt:improve the candidate' >/dev/null
if printf '%s\n' "$out" | grep -F -- '--no-yolo' >/dev/null; then
  echo "agent-tk must not pass --no-yolo" >&2
  exit 1
fi

out=$(cd "$workdir" && TK="$workdir/tk" AVO_MODEL=supervisor-strong AVO_DRIVER_MODEL=grok-4.5 "$agent" . prompt.txt)
printf '%s\n' "$out" | grep -F -- '--model supervisor-strong' >/dev/null
if printf '%s\n' "$out" | grep -F -- '--no-yolo' >/dev/null; then
  echo "agent-tk must not pass --no-yolo" >&2
  exit 1
fi

passdir="$workdir/passdir"
mkdir -p "$passdir"
printf '#!/bin/sh\nexit 0\n' >"$passdir/t.sh"
chmod +x "$passdir/t.sh"
AVO_TEST_CMD="$passdir/t.sh" "$score" "$passdir" | "$lua" "$assertions" score timed-valid
AVO_TEST_CMD="false" "$score" "$passdir" | "$lua" "$assertions" score timed-incorrect
