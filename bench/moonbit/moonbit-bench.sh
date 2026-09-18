#!/bin/sh
# moonbit-bench.sh <file.mbt> — run one MoonBit source as a fresh `moon` package.
#
# `moon` runs packages, not files, so the source is staged as <tmp>/main/main.mbt
# inside a one-package module and executed with `moon run main`. The module
# shape mirrors bench/closed-loop/references-moonbit/.
#
# MOON overrides the CLI (default: `moon`, looked up in ~/.moon/bin too).
set -e
case ":$PATH:" in
  *":$HOME/.moon/bin:"*) ;;
  *) PATH="$HOME/.moon/bin:$PATH"; export PATH ;;
esac
MOON=${MOON:-moon}
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/main"
cat >"$T/moon.mod" <<'EOF'
name = "bench/llm"
version = "0.1.0"
preferred_target = "wasm"
EOF
printf 'pkgtype(kind: "executable")\n' >"$T/main/moon.pkg"
cp "$1" "$T/main/main.mbt"
cd "$T"
exec "$MOON" run main
