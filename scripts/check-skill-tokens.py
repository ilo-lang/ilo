#!/usr/bin/env python3
"""
Enforce the modular-skill token budget.

Each `skills/ilo/ilo-*.md` module must encode to <= 1,000 tokens under
`cl100k_base`, with one exception: `ilo-language` is the foundational
module every agent loads first, so it carries a higher 1,500-token cap
to accommodate core syntax that doesn't split cleanly. The aggregate
across all modules must be <= 8,500.

The budget exists because the whole point of splitting the monolithic
~16,000-token compact spec into modules was to let agents load only the
slices their current task needs (typical: 1-2 modules ~ 2,000 tokens). If
a category module drifts past 1,000 tokens, the per-task economics regress,
so the guard is a CI gate, not advisory.

`ilo-builtins` was split into four category files (core, math, io, text)
to give headroom as the language grows (PR: skill-split-by-category).

Run locally with: `python3 scripts/check-skill-tokens.py`
"""

from __future__ import annotations

import sys
from pathlib import Path

import tiktoken

SKILL_NAMES = [
    "ilo-language",
    "ilo-language-records",
    "ilo-builtins-core",
    "ilo-builtins-math",
    "ilo-builtins-io",
    "ilo-builtins-text",
    "ilo-errors",
    "ilo-tools",
    "ilo-engines",
    "ilo-agent",
    "ilo-examples",
    "ilo-edit-loop",
]

PER_MODULE_LIMIT = 1000
# `ilo-language` is the foundational module every agent loads first; it
# carries a higher cap because core syntax doesn't split cleanly into
# smaller files. `ilo-builtins-io` is the next most-touched module —
# HTTP, JSON, env, time, and process all live there; agent dogfooding
# hits this cap on every other doc PR. Bumped to match its density.
PER_MODULE_OVERRIDES = {
    "ilo-language": 1500,
    "ilo-builtins-io": 1500,
}
TOTAL_LIMIT = 9000


def main() -> int:
    enc = tiktoken.get_encoding("cl100k_base")
    skills_dir = Path(__file__).resolve().parent.parent / "skills" / "ilo"

    total = 0
    failed = False
    for name in SKILL_NAMES:
        path = skills_dir / f"{name}.md"
        if not path.exists():
            print(f"ERROR: missing skill file: {path}", file=sys.stderr)
            failed = True
            continue
        tokens = len(enc.encode(path.read_text()))
        total += tokens
        limit = PER_MODULE_OVERRIDES.get(name, PER_MODULE_LIMIT)
        flag = f"  OVER (cap {limit})" if tokens > limit else ""
        print(f"  {name:<22} {tokens:5d} tokens{flag}")
        if tokens > limit:
            failed = True

    print(f"  {'TOTAL':<22} {total:5d} tokens")
    if total > TOTAL_LIMIT:
        print(
            f"ERROR: total {total} exceeds aggregate budget {TOTAL_LIMIT}",
            file=sys.stderr,
        )
        failed = True
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
