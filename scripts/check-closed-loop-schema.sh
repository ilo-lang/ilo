#!/usr/bin/env bash
# Honest closed-loop schema gate. No model calls, no API key.
#
# Pass: hand-built fixture with thinking_tokens and/or generated_chars.
# Fail: quarantined undivided shape (the 2026-08-03 cell shape).
# New bench/closed-loop-*.json files must pass. historical/ is excluded.
#
# See bench/MANIFESTO-METRIC.md.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

VALIDATOR=(python3 scripts/validate-closed-loop-results.py)
# The validator must not depend on a key. Drop it if the caller has one.
unset ANTHROPIC_API_KEY

fail() {
  echo "FAIL $*" >&2
  exit 1
}

expect_ok() {
  local label="$1"
  shift
  local out
  out="$(mktemp)"
  if "$@" >"$out" 2>&1; then
    echo "PASS $label"
  else
    echo "FAIL $label (expected exit 0)" >&2
    cat "$out" >&2
    rm -f "$out"
    exit 1
  fi
  cat "$out"
  rm -f "$out"
}

expect_fail() {
  local label="$1"
  shift
  local out status
  out="$(mktemp)"
  set +e
  "$@" >"$out" 2>&1
  status=$?
  set -e
  if [[ $status -eq 0 ]]; then
    echo "FAIL $label (expected non-zero)" >&2
    cat "$out" >&2
    rm -f "$out"
    exit 1
  fi
  echo "PASS $label (exit $status)"
  cat "$out"
  rm -f "$out"
}

if [[ -n "${ANTHROPIC_API_KEY-}" ]]; then
  fail "ANTHROPIC_API_KEY is set; this gate must run without it"
fi

echo "== honest fixture =="
expect_ok "honest fixture" "${VALIDATOR[@]}" bench/fixtures/closed-loop-honest.json

echo "== quarantined undivided shape (2026-08-03 cell shape) =="
hist_out="$(mktemp)"
set +e
"${VALIDATOR[@]}" bench/historical/closed-loop-2026-08-03.json >"$hist_out" 2>&1
hist_status=$?
set -e
if [[ $hist_status -eq 0 ]]; then
  cat "$hist_out" >&2
  rm -f "$hist_out"
  fail "historical undivided fixture must fail the honest schema"
fi
undivided="$(grep -c 'not honest:' "$hist_out" || true)"
if [[ "$undivided" -ne 5 ]]; then
  cat "$hist_out" >&2
  rm -f "$hist_out"
  fail "expected 5 undivided cells, found $undivided"
fi
echo "PASS historical undivided fixture (exit $hist_status, $undivided cells)"
cat "$hist_out"
rm -f "$hist_out"

echo "== null thinking_tokens does not count =="
expect_fail "null thinking_tokens" "${VALIDATOR[@]}" bench/fixtures/closed-loop-null-thinking.json

echo "== code_tokens null rule =="
expect_fail "code_tokens null rule" "${VALIDATOR[@]}" bench/fixtures/closed-loop-code-tokens-null-rule.json

echo "== --allow-historical-fail tolerates only the undivided gap =="
expect_ok "allow-historical-fail on undivided fixture" \
  "${VALIDATOR[@]}" --allow-historical-fail bench/historical/closed-loop-2026-08-03.json
expect_fail "allow-historical-fail still rejects null rule" \
  "${VALIDATOR[@]}" --allow-historical-fail bench/fixtures/closed-loop-code-tokens-null-rule.json

echo "== new bench/closed-loop-*.json =="
shopt -s nullglob
new_results=(bench/closed-loop-*.json)
if [[ ${#new_results[@]} -eq 0 ]]; then
  echo "PASS no bench/closed-loop-*.json (historical results stay quarantined)"
else
  expect_ok "new closed-loop results" "${VALIDATOR[@]}" "${new_results[@]}"
fi

echo "OK closed-loop schema gate"
