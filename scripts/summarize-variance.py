#!/usr/bin/env python3
"""summarize-variance.py — aggregate repeat-bench runs into per-language
statistics (mean/median/sd cost, first-attempt success).

Reads the per-repeat output directories written by closed-loop-bench.py
(e.g. bench/variance/r1..r3 or bench/variance-cold/r1..r3) and prints one
summary table per language.

Usage:
  python3 scripts/summarize-variance.py bench/variance r1 r2 r3
  python3 scripts/summarize-variance.py bench/variance-cold r1 r2 r3 --ilo-only
"""

from __future__ import annotations

import json
import statistics as st
import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) < 3:
        print("usage: summarize-variance.py <base-dir> <run-dir>...",
              file=sys.stderr)
        return 2
    base = Path(sys.argv[1])
    runs = sys.argv[2:]

    per_lang: dict[str, dict] = {}
    for run in runs:
        d = base / run
        files = sorted(d.glob("closed-loop-*.json"))
        if not files:
            print(f"skip {run}: no closed-loop JSON in {d}", file=sys.stderr)
            continue
        data = json.loads(files[0].read_text())
        for r in data["results"]:
            lang = r["language"]
            slot = per_lang.setdefault(lang, {"costs": [], "gen": [],
                                              "first": 0, "n": 0})
            slot["costs"].append(r["cost_usd"])
            slot["gen"].append(r["generation_tokens"])
            if r["attempts_to_success"] == 1:
                slot["first"] += 1
            slot["n"] += 1

    print(f"{'lang':<8} {'runs':>4} {'mean $':>9} {'median $':>9} "
          f"{'sd $':>8} {'first-att':>10}")
    print("-" * 56)
    for lang, s in sorted(per_lang.items()):
        c = s["costs"]
        mean = st.mean(c)
        med = st.median(c)
        sd = st.stdev(c) if len(c) > 1 else 0.0
        print(f"{lang:<8} {s['n']:>4} {mean:>9.4f} {med:>9.4f} "
              f"{sd:>8.4f} {s['first']:>6}/{s['n']}")
    print()
    print("Quote medians when the sd approaches the mean — the mean is "
          "tail-driven by retry-thrash runs (see measurements.md).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
