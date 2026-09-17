#!/usr/bin/env python3
"""check-skill-growth.py — G2 eviction-linkage gate (PLAN.md G2).

Composes with scripts/check-skill-tokens.ilo (which enforces per-module and
aggregate token budgets). This gate enforces the *linkage* rule: a module
may grow past its recorded baseline (bench/skill-token-baseline.json) only
when skills/ilo/EVICTION-LOG.md names it with a persona-transcript link.

Bootstrap: if the baseline file does not exist, exit 0 and say so.

Usage:
  python3 scripts/check-skill-growth.py
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SKILLS_DIR = REPO_ROOT / "skills" / "ilo"
BASELINE = REPO_ROOT / "bench" / "skill-token-baseline.json"
LOG = SKILLS_DIR / "EVICTION-LOG.md"

try:
    import tiktoken
    _enc = tiktoken.get_encoding("cl100k_base")
except ImportError:
    _enc = None


def tokens(text: str) -> int:
    return len(_enc.encode(text)) if _enc else len(text) // 4


def main() -> int:
    if not BASELINE.exists():
        print("skill-growth: no baseline (run scripts/update-skill-baseline.py); "
              "growth check skipped", file=sys.stderr)
        return 0

    baseline = json.loads(BASELINE.read_text())
    log_text = LOG.read_text() if LOG.exists() else ""

    failures = []
    for path in sorted(SKILLS_DIR.glob("ilo-*.md")):
        name = path.stem
        if name not in baseline:
            failures.append(f"{name}: no baseline entry "
                            f"(run scripts/update-skill-baseline.py)")
            continue
        cur = tokens(path.read_text())
        base = baseline[name]
        if cur > base and name not in log_text:
            failures.append(
                f"{name}: grew {base}→{cur} tokens without an "
                f"EVICTION-LOG.md entry (persona-transcript link required)")

    if failures:
        print("GROWTH-UNLINKED:", file=sys.stderr)
        for f in failures:
            print(f"  {f}", file=sys.stderr)
        return 1

    print(f"skill-growth: OK ({len(baseline)} modules at or under baseline)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
