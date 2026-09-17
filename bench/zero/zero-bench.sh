#!/bin/sh
# zero-bench.sh <file.0> — run a Zero .0 projection through a fresh project.
set -e
ZERO=${ZERO:-/tmp/agentlangs/zerolang/.zero/bin/zero}
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/src"
cp "$1" "$T/src/main.0"
cd "$T"
"$ZERO" init >/dev/null 2>&1
"$ZERO" import src/main.0 >/dev/null 2>&1
exec "$ZERO" run
