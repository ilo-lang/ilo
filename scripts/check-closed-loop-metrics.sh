#!/usr/bin/env bash
# Closed-loop metric gate. No API key, no network, no harness run.
set -euo pipefail
cd "$(dirname "$0")/.."
unset ANTHROPIC_API_KEY || true

val=(python3 scripts/validate-closed-loop-results.py)

echo "== honest fixtures (must pass) =="
"${val[@]}" bench/fixtures/honest-closed-loop-cell.json
"${val[@]}" bench/fixtures/chars-only-cell.json

expect_fail() {
  local label=$1
  shift
  set +e
  "${val[@]}" "$@"
  local rc=$?
  set -e
  if [[ $rc -eq 0 ]]; then
    echo "FAIL: $label was accepted" >&2
    exit 1
  fi
  if [[ $rc -ne 1 ]]; then
    echo "FAIL: $label exited $rc (expected 1)" >&2
    exit 1
  fi
  echo "ok: $label rejected"
}

echo "== undivided / dishonest fixtures (must fail) =="
expect_fail "historical undivided shape" \
  bench/historical/closed-loop-2026-08-03.undivided-shape.json
expect_fail "null thinking_tokens" \
  bench/fixtures/null-thinking-not-honest.json
expect_fail "code_tokens with unknown thinking" \
  bench/fixtures/unknown-thinking-code-tokens.json

echo "== --allow-historical-fail does not excuse other files =="
expect_fail "non-historical null thinking under --allow-historical-fail" \
  --allow-historical-fail \
  bench/fixtures/null-thinking-not-honest.json

echo "== Aug-03 basename is historical even outside bench/historical/ =="
tmp=$(mktemp -d)
cp bench/historical/closed-loop-2026-08-03.undivided-shape.json \
  "$tmp/closed-loop-2026-08-03.json"
expect_fail "Aug-03 basename" "$tmp/closed-loop-2026-08-03.json"
"${val[@]}" --allow-historical-fail \
  "$tmp/closed-loop-2026-08-03.json" \
  bench/fixtures/honest-closed-loop-cell.json
rm -rf "$tmp"

echo "== mixed run: historical allowed, honest still required =="
"${val[@]}" --allow-historical-fail \
  bench/historical/closed-loop-2026-08-03.undivided-shape.json \
  bench/fixtures/honest-closed-loop-cell.json

echo "== published bench/closed-loop-*.json must pass =="
shopt -s nullglob
published=(bench/closed-loop-*.json)
if ((${#published[@]})); then
  "${val[@]}" "${published[@]}"
else
  echo "no bench/closed-loop-*.json (historical lives under bench/historical/)"
fi

echo "closed-loop metric gate ok"
