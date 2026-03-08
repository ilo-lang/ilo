#!/bin/bash
# Benchmark: Graph vs Non-Graph approach for agent context loading
# Scenario: An agent needs to modify the `build-order` function in ecommerce.ilo

set -e

ILO="cargo run --quiet --"
FILE="examples/ecommerce.ilo"

echo "================================================================="
echo "BENCHMARK: Graph vs Non-Graph Context Loading"
echo "Scenario: Agent needs to modify 'build-order' function"
echo "================================================================="
echo ""

# --- NON-GRAPH: Agent loads the entire file ---
echo "--- APPROACH 1: Load entire file (no graph) ---"
FULL_SOURCE=$(cat "$FILE")
FULL_TOKENS=$(echo "$FULL_SOURCE" | wc -w | tr -d ' ')
FULL_LINES=$(echo "$FULL_SOURCE" | wc -l | tr -d ' ')
FULL_CHARS=$(echo "$FULL_SOURCE" | wc -c | tr -d ' ')
echo "Lines:  $FULL_LINES"
echo "Words:  $FULL_TOKENS (≈ tokens)"
echo "Chars:  $FULL_CHARS"
echo ""

# --- GRAPH: Agent queries just build-order + deps (signatures only) ---
echo "--- APPROACH 2: ilo graph --fn build-order (sigs only) ---"
FN_OUTPUT=$($ILO graph "$FILE" --fn build-order 2>/dev/null)
FN_TOKENS=$(echo "$FN_OUTPUT" | wc -w | tr -d ' ')
FN_CHARS=$(echo "$FN_OUTPUT" | wc -c | tr -d ' ')
echo "Words:  $FN_TOKENS (≈ tokens)"
echo "Chars:  $FN_CHARS"
echo ""

# --- GRAPH: Agent queries build-order + full subgraph ---
echo "--- APPROACH 3: ilo graph --fn build-order --subgraph (full deps) ---"
SUB_OUTPUT=$($ILO graph "$FILE" --fn build-order --subgraph 2>/dev/null)
SUB_TOKENS=$(echo "$SUB_OUTPUT" | wc -w | tr -d ' ')
SUB_CHARS=$(echo "$SUB_OUTPUT" | wc -c | tr -d ' ')
echo "Words:  $SUB_TOKENS (≈ tokens)"
echo "Chars:  $SUB_CHARS"
echo ""

# --- GRAPH: Budget-constrained ---
echo "--- APPROACH 4: ilo graph --fn build-order --subgraph --budget 30 ---"
BUD_OUTPUT=$($ILO graph "$FILE" --fn build-order --subgraph --budget 30 2>/dev/null)
BUD_TOKENS=$(echo "$BUD_OUTPUT" | wc -w | tr -d ' ')
BUD_CHARS=$(echo "$BUD_OUTPUT" | wc -c | tr -d ' ')
echo "Words:  $BUD_TOKENS (≈ tokens)"
echo "Chars:  $BUD_CHARS"
echo ""

# --- GRAPH: Reverse query (what breaks if I change vld-addr?) ---
echo "--- APPROACH 5: ilo graph --fn vld-addr --reverse ---"
REV_OUTPUT=$($ILO graph "$FILE" --fn vld-addr --reverse 2>/dev/null)
REV_TOKENS=$(echo "$REV_OUTPUT" | wc -w | tr -d ' ')
REV_CHARS=$(echo "$REV_OUTPUT" | wc -c | tr -d ' ')
echo "Words:  $REV_TOKENS (≈ tokens)"
echo "Chars:  $REV_CHARS"
echo ""

# --- Summary ---
echo "================================================================="
echo "SUMMARY"
echo "================================================================="
echo ""
echo "Approach                          Words(≈tokens)  Reduction"
echo "─────────────────────────────────────────────────────────────"
printf "1. Full file (no graph)           %-15s baseline\n" "$FULL_TOKENS"
printf "2. --fn (sigs only)               %-15s %d%%\n" "$FN_TOKENS" "$(( (FULL_TOKENS - FN_TOKENS) * 100 / FULL_TOKENS ))"
printf "3. --fn --subgraph (full deps)    %-15s %d%%\n" "$SUB_TOKENS" "$(( (FULL_TOKENS - SUB_TOKENS) * 100 / FULL_TOKENS ))"
printf "4. --fn --subgraph --budget 30    %-15s %d%%\n" "$BUD_TOKENS" "$(( (FULL_TOKENS - BUD_TOKENS) * 100 / FULL_TOKENS ))"
printf "5. --fn --reverse                 %-15s %d%%\n" "$REV_TOKENS" "$(( (FULL_TOKENS - REV_TOKENS) * 100 / FULL_TOKENS ))"
echo ""
echo "Note: 'Words' is a rough proxy for LLM tokens."
echo "JSON overhead inflates graph output — the source content"
echo "itself is much smaller than the full file."
