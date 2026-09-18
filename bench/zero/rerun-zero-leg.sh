#!/bin/sh
# rerun-zero-leg.sh — one-shot rerun of the Zero comparator leg.
#
# Prereqs:
#   1. A funded OpenAI-compatible key (DeepSeek or otherwise) exported as
#      DEEPSEEK_API_KEY (or adjust --model/keys below).
#   2. Zero built: make -C /tmp/agentlangs/zerolang/native/zero-c
#   3. ILO built: cargo build --release
#
# Blocks: see bench/measurements.md "Comparator status" — the 2026-09-17
# attempt ended with DeepSeek balance -$0.42 before any LLM call succeeded.
set -e
cd "$(dirname "$0")/.."

ZERO_HOME=${ZERO_HOME:-/tmp/agentlangs/zerolang}
export ZERO="$ZERO_HOME/.zero/bin/zero"
export PATH="$ZERO:$PATH"

# Zero's agent-facing docs at parity with ilo's curated spec
zero skills get language > /tmp/zero-docs-language.md
zero skills get stdlib  > /tmp/zero-docs-stdlib.md
cat /tmp/zero-docs-language.md /tmp/zero-docs-stdlib.md > /tmp/zero-docs.md

for arm in warm cold; do
  python3 scripts/closed-loop-bench.py \
      --model "$MODEL" \
      --cache $arm \
      --align \
      --context curated \
      --lang2-name zero \
      --lang2-bin bench/zero/zero-bench.sh \
      --lang2-ext .0 \
      --lang2-docs /tmp/zero-docs.md \
      --ilo ./target/release/ilo
done

echo "Zero leg complete (warm + cold)."
