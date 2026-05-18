#!/usr/bin/env bash
# Closed-loop benchmark — reproducible execution script.
#
# Usage:
#   ./run_benchmark.sh              # full live matrix (requires ANTHROPIC_API_KEY)
#   ./run_benchmark.sh --mock       # mock LLM (no API key, validates plumbing)
#   ./run_benchmark.sh --sample     # small sample matrix (quick check)

set -euo pipefail
cd "$(dirname "$0")"

MODE="--full"
SEEDS=3
BUDGET=300

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mock)   MODE="--mock --sample" ;;
    --sample) MODE="--mock --sample" ;;
    --live)   MODE="--full" ;;
    --seeds)  shift; SEEDS="$1" ;;
    --budget) shift; BUDGET="$1" ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
  shift
done

mkdir -p results/raw charts data

if [[ "$MODE" == "--full" ]] && [[ -z "${ANTHROPIC_API_KEY:-}" ]]; then
  echo "ANTHROPIC_API_KEY is not set." >&2
  echo "Either:" >&2
  echo "  export ANTHROPIC_API_KEY=sk-ant-...   (for live runs)" >&2
  echo "  ./run_benchmark.sh --mock              (for offline plumbing test)" >&2
  exit 2
fi

echo "==> Verifying tools"
command -v python3 >/dev/null || { echo "python3 missing"; exit 2; }
command -v ilo     >/dev/null || echo "warning: ilo not on PATH — ilo variants will be skipped"
command -v zero    >/dev/null || echo "warning: zero not on PATH — zero variant may be skipped"

echo "==> Running driver in mode: $MODE (seeds=$SEEDS budget=\$$BUDGET)"
python3 driver.py $MODE --seeds "$SEEDS" --budget "$BUDGET"

echo "==> Generating charts"
python3 charts.py

echo
echo "Done. Outputs:"
echo "  results/aggregated.json   (matrix data)"
echo "  data/aggregated.csv       (flat CSV)"
echo "  charts/*.{png,svg}        (4 canonical charts)"
echo "  charts/summary.txt        (text summary)"
