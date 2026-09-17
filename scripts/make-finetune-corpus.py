#!/usr/bin/env python3
"""
make-finetune-corpus.py — seed the Phase 4 "train ilo into weights" work
(PLAN.md Phase 4: the spec-loading term goes to zero for a fine-tuned model).

Emits a JSONL chat corpus from examples/ plus the bundled skill modules:

  {"system": <skill spec>, "messages": [{"user": <task>, "assistant": <source>}]}

Per example:
  - header comment block (`-- ...`) becomes the task description; when absent,
    the filename slug does.
  - the remaining source becomes the assistant completion.
  - examples that fail `ilo check` are skipped (counted, not silently kept).

The corpus is the raw material, not a finished fine-tune: curation (dedupe,
difficulty tiers, held-out split) is a follow-up.  Usage:

  python3 scripts/make-finetune-corpus.py [--ilo ./target/release/ilo] \
      [--out bench/finetune-corpus.jsonl] [--context full|core]
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
EXAMPLES_DIR = REPO_ROOT / "examples"
SKILLS_DIR = REPO_ROOT / "skills" / "ilo"

FULL_MODS = ["ilo-language", "ilo-builtins-core", "ilo-builtins-text",
             "ilo-builtins-math", "ilo-builtins-io"]
CORE_MODS = ["ilo-language", "ilo-builtins-core"]

HEADER_RE = re.compile(r"^--.*\n?", re.MULTILINE)


def skill_context(mods: list[str]) -> str:
    parts = []
    for m in mods:
        p = SKILLS_DIR / f"{m}.md"
        if p.exists():
            parts.append(p.read_text())
    return "\n\n".join(parts)


def split_example(text: str) -> tuple[str, str]:
    """Split an example file into (header-comment task text, source code)."""
    lines = text.splitlines(keepends=True)
    header: list[str] = []
    body_start = 0
    for i, line in enumerate(lines):
        if line.startswith("--"):
            header.append(line[2:].strip())
            body_start = i + 1
        elif line.strip() == "":
            body_start = i + 1
        else:
            break
    task = "\n".join(h for h in header if h).strip()
    source = "".join(lines[body_start:]).strip()
    return task, source


def slug_to_task(name: str) -> str:
    words = re.sub(r"^\d+", "", Path(name).stem)
    words = words.replace("-", " ").replace("_", " ").strip()
    return f"Write an ilo program: {words}."


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--ilo", default=os.environ.get("ILO", "ilo"),
                    help="ilo binary used to verify each example.")
    ap.add_argument("--out", default=str(BENCH_OUT()),
                    help="Output JSONL path.")
    ap.add_argument("--context", choices=["full", "core"], default="core",
                    help="Skill context baked into each system prompt.")
    args = ap.parse_args()

    mods = CORE_MODS if args.context == "core" else FULL_MODS
    system = skill_context(mods)

    kept, skipped_check, skipped_empty = [], 0, 0
    for path in sorted(EXAMPLES_DIR.glob("*.ilo")):
        raw = path.read_text()
        task, source = split_example(raw)
        if not source:
            skipped_empty += 1
            continue
        if not task:
            task = slug_to_task(path.name)
        # Only keep examples that still verify against the current compiler.
        with tempfile_write(source) as tmp:
            r = subprocess.run([args.ilo, "check", tmp],
                               capture_output=True, text=True, timeout=20)
        if r.returncode != 0:
            skipped_check += 1
            continue
        kept.append({
            "meta": {"example": path.name, "context": args.context},
            "system": system,
            "messages": [
                {"role": "user", "content": task},
                {"role": "assistant", "content": source},
            ],
        })

    out = Path(args.out)
    out.write_text("".join(json.dumps(r) + "\n" for r in kept))
    print(f"corpus: {len(kept)} examples -> {out}")
    print(f"skipped: {skipped_check} failed ilo check, {skipped_empty} empty")
    print(f"system prompt tokens ~{len(system) // 4} (chars/{4})")
    return 0


def BENCH_OUT() -> Path:
    return REPO_ROOT / "bench" / "finetune-corpus.jsonl"


import contextlib


def tempfile_write(content: str):
    """Tiny context manager: write content to a temp file, yield the path."""
    import tempfile

    class _T:
        def __enter__(self):
            self.f = tempfile.NamedTemporaryFile(
                suffix=".ilo", mode="w", delete=False)
            self.f.write(content)
            self.f.close()
            return self.f.name

        def __exit__(self, *a):
            os.unlink(self.f.name)
            return False

    return _T()


if __name__ == "__main__":
    sys.exit(main())
