#!/usr/bin/env python3
"""ilo-harness.py — emit the cache-aligned ilo system prompt, and assert it is
byte-stable (PLAN.md G4).

The aligned shape, in first-byte order:

  1. ilo skill spec (curated modules, concatenated byte-stable per version)
  2. tools / harness contract (static)
  3. task text (volatile, last)

Stability contract: for a given ilo version (^26.5 pragma) the bytes from
position 0 through the end of block 2 are identical across runs, machines,
and processes. The provider prefix cache therefore survives every call, and
`^26.5` is the only event that can invalidate it.

Usage:
  python3 scripts/ilo-harness.py                      # print the prompt
  python3 scripts/ilo-harness.py --check              # verify stability (exit 0/1)
  python3 scripts/ilo-harness.py --task "Write ..."   # prompt + task appended
  python3 scripts/ilo-harness.py --context full       # pre-G2 module set
"""

from __future__ import annotations

import argparse
import hashlib
import os
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SKILLS_DIR = REPO_ROOT / "skills" / "ilo"
REFERENCE_DIR = REPO_ROOT / "docs" / "reference"

CURATED = ["ilo-language", "ilo-builtins-core", "ilo-builtins-sig"]
FULL_EXTRA = ["ilo-builtins-io", "ilo-builtins-text", "ilo-builtins-math"]

TOOLS_CONTRACT = """\
# Harness contract

- Emit exactly one ilo program per response. No prose, no markdown fences.
- The program's first line must be `-- run: main`.
- If the task names expected output, add `-- out: <expected>` on line 2.
- Errors you receive are verifier diagnostics (ILO- codes) or runtime
  output. Fix the named span; do not restate the program.
"""


def load_modules(mods: list[str]) -> str:
    parts = []
    for m in mods:
        for base in (SKILLS_DIR, REFERENCE_DIR):
            p = base / f"{m}.md"
            if p.exists():
                parts.append(p.read_text())
                break
        else:
            print(f"ERROR: module not found: {m}", file=sys.stderr)
            sys.exit(2)
    return "\n\n".join(parts)


def build_prompt(context: str) -> str:
    spec = load_modules(CURATED + (FULL_EXTRA if context == "full" else []))
    return f"{spec}\n\n{TOOLS_CONTRACT}"


def stability_digest(prompt: str) -> str:
    return hashlib.sha256(prompt.encode()).hexdigest()[:16]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--context", choices=["curated", "full"], default="curated")
    ap.add_argument("--task", default=None, help="Append a task as block 3.")
    ap.add_argument("--check", action="store_true",
                    help="Build the prompt twice (fresh module loads) and "
                         "verify the spec+tools prefix is byte-identical.")
    ap.add_argument("--digest", action="store_true",
                    help="Print the stability digest instead of the prompt.")
    args = ap.parse_args()

    prompt = build_prompt(args.context)

    if args.check:
        again = build_prompt(args.context)
        if prompt != again:
            print("FAIL: prefix not byte-stable across loads", file=sys.stderr)
            return 1
        d = stability_digest(prompt)
        expected_file = REPO_ROOT / "bench" / f"harness-digest-{args.context}.txt"
        if expected_file.exists():
            expected = expected_file.read_text().strip()
            if expected != d:
                print(
                    f"FAIL: digest changed since last record\n"
                    f"  expected: {expected}\n  actual:   {d}\n"
                    f"If the spec changed intentionally, update "
                    f"{expected_file} and bump the ^pragma.",
                    file=sys.stderr,
                )
                return 1
        else:
            expected_file.write_text(d + "\n")
            print(f"Digest recorded: {d} ({expected_file})", file=sys.stderr)
        print(f"OK: byte-stable, digest {d}", file=sys.stderr)
        return 0

    if args.digest:
        print(stability_digest(prompt))
        return 0

    out = prompt
    if args.task:
        out += f"\n# Task\n\n{args.task}\n"
    sys.stdout.write(out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
