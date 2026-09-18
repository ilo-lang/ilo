#!/bin/sh
# zero-bench.sh <file.0> — run a Zero .0 projection through a fresh project.
#
# --selfcheck runs the simple-function reference (expected: 55) end to end, so
# the matrix skips the leg instead of scoring a broken toolchain.
set -e
ZERO=${ZERO:-/tmp/agentlangs/zerolang/.zero/bin/zero}
if [ "${1:-}" = "--selfcheck" ]; then
  ref="$(cd "$(dirname "$0")/.." && pwd)/closed-loop/references-zero/simple-function.0"
  out=$("$0" "$ref")
  [ "$out" = "55" ] || { echo "zero selfcheck failed: got '$out', want '55'" >&2; exit 1; }
  exit 0
fi
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/src"
cp "$1" "$T/src/main.0"
cd "$T"
"$ZERO" init >/dev/null 2>&1
"$ZERO" import src/main.0 >/dev/null 2>&1
exec "$ZERO" run
