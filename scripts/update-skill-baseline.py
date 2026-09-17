#!/usr/bin/env python3
"""update-skill-baseline.py — record current per-module token sizes as the
G2 eviction-linkage baseline.

Run this ONLY after an intentional, EVICTION-LOG-linked spec change: the
gate (scripts/check-skill-tokens.ilo) fails any module that grows past its
recorded baseline without a matching EVICTION-LOG.md entry.

Usage:
  python3 scripts/update-skill-baseline.py            # record current sizes
  python3 scripts/update-skill-baseline.py --show     # print without writing
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SKILLS_DIR = REPO_ROOT / "skills" / "ilo"
OUT = REPO_ROOT / "bench" / "skill-token-baseline.json"

try:
    import tiktoken
    _enc = tiktoken.get_encoding("cl100k_base")
except ImportError:
    _enc = None


def tokens(text: str) -> int:
    if _enc is not None:
        return len(_enc.encode(text))
    return len(text) // 4


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--show", action="store_true")
    args = ap.parse_args()

    report = {}
    for path in sorted(SKILLS_DIR.glob("ilo-*.md")):
        report[path.stem] = tokens(path.read_text())

    body = json.dumps(report, indent=2, sort_keys=True)
    if args.show:
        print(body)
        return 0
    OUT.write_text(body + "\n")
    print(f"baseline written: {OUT}")
    for k, v in report.items():
        print(f"  {k:<26} {v}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
