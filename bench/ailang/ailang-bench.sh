#!/bin/sh
# ailang-bench.sh <file.ail> — run one AILANG source through a fresh module root.
#
# AILANG resolves the `module bench/<name>` declaration relative to a module
# root, so a bare `ailang file.ail` fails with LDR001 (module not found). The
# source is staged at <tmp>/<module-path>.ail and run from <tmp>, mirroring how
# the hand-authored references in bench/closed-loop/references-ailang/ were run.
#
# Capabilities: the reference set needs IO everywhere and Env for
# tool-interaction (see the `-- Run:` line in each reference), so this leg
# grants the union for every task rather than switching per task.
#
# AILANG overrides the binary (default: bench/comparators/src/ailang/bin/ailang).
set -e
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
AILANG=${AILANG:-$ROOT/bench/comparators/src/ailang/bin/ailang}
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT

src=$1
mod=$(sed -n 's/^module[[:space:]]\{1,\}\([^[:space:]]*\).*/\1/p' "$src" | head -1)
rel=${mod:-bench/$(basename "$src" .ail)}
mkdir -p "$T/$(dirname "$rel")"
cp "$src" "$T/$rel.ail"
cd "$T"
exec "$AILANG" run -quiet --caps IO,Env "$rel.ail"
