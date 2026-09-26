#!/usr/bin/env bash
# rustc PGO + LLVM PGSO lane for `tk`.
#
# Same LLVM PGSO idea as vercel-labs/fx (`scripts/pgso`, `-Doptimize=ReleaseSafe`
# then profile-use so hot code stays speed-oriented and cold functions compile
# for size). Rust has no Zig PGSO driver; this uses rustc's documented PGO
# flags plus LLVM `--pgso` via `-C llvm-args`.
#
# References:
#   https://doc.rust-lang.org/rustc/profile-guided-optimization.html
#   https://github.com/Kobzol/cargo-pgo (CI shape; we call rustc flags directly)
#   https://github.com/vercel-labs/fx/blob/main/scripts/pgso/README.md
#
# Usage:
#   scripts/pgo/pgo.sh [--target TRIPLE] [--out DIR] [--skip-pgso]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TUI="$ROOT/ui/tui"
TARGET=""
OUT=""
SKIP_PGSO=0

usage() {
  cat <<'H'
scripts/pgo/pgo.sh — instrument, train, and rebuild tk with PGO + LLVM PGSO

Usage:
  scripts/pgo/pgo.sh [--target TRIPLE] [--out DIR] [--skip-pgso]

Writes the optimized binary to:
  ui/tui/target/<triple>/release/tk

Env:
  CARGO                 cargo binary (default: cargo)
  LLVM_PROFDATA         llvm-profdata override
H
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      TARGET="${2:?}"
      shift 2
      ;;
    --out)
      OUT="${2:?}"
      shift 2
      ;;
    --skip-pgso)
      SKIP_PGSO=1
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

CARGO="${CARGO:-cargo}"
HOST="$(rustc -vV | awk '/^host:/{print $2}')"
if [[ -z "$TARGET" ]]; then
  TARGET="$HOST"
fi
if [[ "$TARGET" != "$HOST" ]]; then
  echo "pgo: --target $TARGET is not the host ($HOST); cannot train a cross-compiled binary" >&2
  exit 1
fi
if [[ -z "$OUT" ]]; then
  OUT="$TUI/target/pgo/$TARGET"
fi
mkdir -p "$OUT/raw"

llvm_profdata() {
  if [[ -n "${LLVM_PROFDATA:-}" ]]; then
    echo "$LLVM_PROFDATA"
    return
  fi
  local sysroot bin
  sysroot="$(rustc --print sysroot)"
  bin="$sysroot/lib/rustlib/$HOST/bin/llvm-profdata"
  if [[ -x "$bin" ]]; then
    echo "$bin"
    return
  fi
  if command -v llvm-profdata >/dev/null 2>&1; then
    command -v llvm-profdata
    return
  fi
  echo "scripts/pgo/pgo.sh: llvm-profdata not found (rustup component add llvm-tools-preview)" >&2
  exit 1
}

PROFDATA="$OUT/merged.profdata"
BIN="$TUI/target/$TARGET/release/tk"
# --force-pgso: CLI-only training has a small working set; without this LLVM
# may skip PGSO (--pgso-lwss-only). Same idea as fx compiling cold IR for size.
PGSO_ARGS="-Cllvm-args=--pgso -Cllvm-args=--force-pgso -Cllvm-args=--pgso-cold-code-only-for-instr-pgo -Cllvm-args=-pgo-warn-missing-function"
if [[ "$SKIP_PGSO" -eq 1 ]]; then
  PGSO_ARGS="-Cllvm-args=-pgo-warn-missing-function"
fi

echo "pgo: instrument target=$TARGET"
(
  cd "$TUI"
  CARGO_INCREMENTAL=0 RUSTFLAGS="-Cprofile-generate=$OUT/raw" \
    "$CARGO" build --locked --release --target "$TARGET"
)

echo "pgo: train $BIN"
LLVM_PROFILE_FILE="$OUT/raw/tk-%p-%m.profraw" \
  bash "$ROOT/scripts/pgo/train.sh" "$BIN"

shopt -s nullglob
raws=("$OUT/raw"/*.profraw)
if [[ ${#raws[@]} -eq 0 ]]; then
  echo "pgo: no .profraw files written under $OUT/raw" >&2
  exit 1
fi

echo "pgo: merge profiles"
"$(llvm_profdata)" merge -o "$PROFDATA" "${raws[@]}"
if [[ ! -s "$PROFDATA" ]]; then
  echo "pgo: empty $PROFDATA" >&2
  exit 1
fi

echo "pgo: optimize (pgso=$([[ $SKIP_PGSO -eq 0 ]] && echo on || echo off))"
(
  cd "$TUI"
  # cargo:rustc-env / build scripts must not see profile-use; --target keeps
  # RUSTFLAGS off the host build-script rustc (rustc book PGO workflow).
  CARGO_INCREMENTAL=0 RUSTFLAGS="-Cprofile-use=$PROFDATA $PGSO_ARGS" \
    "$CARGO" build --locked --release --target "$TARGET"
)

if [[ ! -x "$BIN" ]]; then
  echo "pgo: missing optimized binary $BIN" >&2
  exit 1
fi

"$BIN" --version >/dev/null
"$BIN" --help >/dev/null
"$BIN" exec --help >/dev/null 2>&1
"$BIN" acp --help >/dev/null 2>&1

echo "pgo: binary=$BIN bytes=$(wc -c <"$BIN" | tr -d ' ') profdata=$PROFDATA"
