#!/usr/bin/env bash
# check-dispatch-arms.sh
#
# Lints the `call_function` dispatch in src/interpreter/mod.rs.
# Fires if any `if builtin == Some(Builtin::...)` block inside call_function
# exceeds THRESHOLD lines.
#
# Background: large inline arms in call_function inflate the debug-build
# stack frame and cause stack-overflow in deep-recursion tests (ILO-339).
# Engineers should extract oversized arms into `#[inline(never)]` helpers.
#
# Usage:
#   bash scripts/check-dispatch-arms.sh [--threshold N]
#   Default threshold: 40 lines.

set -euo pipefail

THRESHOLD=40
FILE="src/interpreter/mod.rs"
TARGET_FN="call_function"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --threshold) THRESHOLD="$2"; shift 2 ;;
    *) echo "Unknown argument: $1" >&2; exit 1 ;;
  esac
done

if [[ ! -f "$FILE" ]]; then
  echo "ERROR: $FILE not found. Run from the repository root." >&2
  exit 1
fi

# --- locate call_function boundaries ---
fn_start=$(grep -n "^fn ${TARGET_FN}(" "$FILE" | head -1 | cut -d: -f1)
if [[ -z "$fn_start" ]]; then
  echo "ERROR: could not find 'fn ${TARGET_FN}(' in $FILE" >&2
  exit 1
fi

# Find the next top-level fn after call_function to bound our search
fn_end=$(awk -v start="$fn_start" 'NR>start && /^fn [a-z]/{print NR; exit}' "$FILE")
if [[ -z "$fn_end" ]]; then
  fn_end=$(wc -l < "$FILE")
fi

# --- extract line numbers of each `if builtin ==` arm inside call_function ---
arm_lines=$(awk -v s="$fn_start" -v e="$fn_end" \
  'NR>=s && NR<=e && /^    if builtin == /{print NR}' "$FILE")

if [[ -z "$arm_lines" ]]; then
  echo "No 'if builtin ==' arms found in ${TARGET_FN} — nothing to check."
  exit 0
fi

# Build array of arm start line numbers, append fn_end as sentinel
mapfile -t starts <<< "$arm_lines"
starts+=("$fn_end")

fail=0
max_arm=0
max_arm_start=0

for (( i=0; i<${#starts[@]}-1; i++ )); do
  arm_s=${starts[$i]}
  arm_e=${starts[$((i+1))]}
  arm_len=$(( arm_e - arm_s ))

  if (( arm_len > max_arm )); then
    max_arm=$arm_len
    max_arm_start=$arm_s
  fi

  if (( arm_len > THRESHOLD )); then
    # Extract the builtin name for a helpful message
    arm_label=$(sed -n "${arm_s}p" "$FILE" | grep -oP 'Builtin::\w+' | head -1)
    echo "FAIL: ${FILE}:${arm_s}: arm '${arm_label}' is ${arm_len} lines (threshold: ${THRESHOLD})."
    echo "      Extract the body into a #[inline(never)] helper function to reduce"
    echo "      call_function's stack frame and prevent debug-build stack overflows."
    fail=1
  fi
done

if (( fail == 0 )); then
  echo "OK: all ${#starts[@]} dispatch arms in ${TARGET_FN} are within ${THRESHOLD}-line threshold."
  echo "    (largest arm: ${max_arm} lines at line ${max_arm_start})"
fi

exit $fail
