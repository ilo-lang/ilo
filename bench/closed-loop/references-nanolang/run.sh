#!/usr/bin/env bash
# Usage: ./run.sh <task-id>
# NanoLang is a C transpiler: nanoc drives transpile -> cc -> link in one step,
# then we execute the produced binary.
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
NANOC="${NANOC:-/tmp/ilo/bench/comparators/src/nanolang/bin/nanoc}"
task="$1"
mkdir -p "$DIR/.build"
bin="$DIR/.build/$task"
"$NANOC" "$DIR/$task.nano" -o "$bin"
exec "$bin"
