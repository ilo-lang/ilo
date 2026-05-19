#!/usr/bin/env bash
# Run every *.ilo in this dir through every public engine (vm/jit/aot) and
# print a Markdown matrix. The tree-walker column was dropped when --run-tree
# was removed from the public CLI in the 0.12.x soft-deprecation; the
# tree-walker stays in-tree as the runtime for HOF callbacks that VM/Cranelift
# bail to, so VM coverage transitively exercises it. Each file's expected
# output is read from any header line(s) of the form `-- expected: <text>`;
# multiple lines are joined with newlines.
#
# Env: ILO_BIN (default: ./target/release/ilo)
#      ILO_AUDIT_NET=1 to include http-* tests (skipped by default)

set -u
DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$DIR/../.." && pwd)"
ILO="${ILO_BIN:-$ROOT/target/release/ilo}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

if [ ! -x "$ILO" ]; then
  echo "ilo binary not found at $ILO; build with cargo build --release --features cranelift" >&2
  exit 1
fi

# Engine invocations
run_vm()   { "$ILO" --run-vm   "$1" 2>&1; }
run_jit()  { "$ILO" --jit      "$1" 2>&1; }
run_aot()  {
  local src="$1"
  local out="$TMP/aot_$(basename "$src" .ilo)"
  # AOT picks the FIRST function as entry by default which is rarely `main`;
  # explicitly pass `main` so we test the obvious user-facing path.
  "$ILO" compile "$src" -o "$out" main >/dev/null 2>"$TMP/aot.err"
  if [ ! -x "$out" ]; then
    head -3 "$TMP/aot.err" | grep -v '^ld:'
    return 1
  fi
  "$out" 2>&1
}

cell() {
  local actual="$1" expected="$2" exitcode="$3"
  if [ "$exitcode" = "0" ] && [ "$actual" = "$expected" ]; then
    echo "OK"
  else
    local snippet
    snippet=$(echo "$actual" | head -1 | cut -c1-60 | tr '|' '/')
    echo "FAIL: $snippet"
  fi
}

printf "| File | Feature | VM | JIT | AOT |\n"
printf "|---|---|---|---|---|\n"

for f in "$DIR"/*.ilo; do
  fname=$(basename "$f")
  feature=$(grep -m1 '^-- feature:' "$f" | sed 's/^-- feature: //')
  # Join all `-- expected:` lines with newlines.
  expected=$(grep '^-- expected:' "$f" | sed 's/^-- expected: //' | awk 'NR==1{printf "%s",$0; next} {printf "\n%s",$0}')

  case "$fname" in
    33-http-*|3[4-9]-http-*) if [ "${ILO_AUDIT_NET:-0}" != "1" ]; then
        printf "| %s | %s | skip | skip | skip |\n" "$fname" "$feature"
        continue
      fi;;
  esac

  ov=$(run_vm   "$f"); ev=$?
  oj=$(run_jit  "$f"); ej=$?
  oa=$(run_aot  "$f"); ea=$?

  cv=$(cell "$ov" "$expected" "$ev")
  cj=$(cell "$oj" "$expected" "$ej")
  ca=$(cell "$oa" "$expected" "$ea")

  printf "| %s | %s | %s | %s | %s |\n" "$fname" "$feature" "$cv" "$cj" "$ca"
done
