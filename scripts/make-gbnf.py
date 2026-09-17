#!/usr/bin/env python3
"""make-gbnf.py — generate an ilo GBNF grammar from the probed transitions.

Reads the v2 constrain artifact (bench/constrain-masks-probed.json,
schemaVersion 2) and emits a llama.cpp GBNF grammar whose states are the
probed two-token contexts: state(prev2, prev1) accepts exactly the tokens
the validity oracle accepted at that context. Regenerated from the parser
on every run — drift-free by construction.

Caveats (docs/constrain-design.md):
  - Conservative: unobserved-but-valid continuations are absent (the 5
    published oracle anomalies are chain-recovery states).
  - Dead-end states can occur for probe-only continuations; generation
    halts there (rare, corpus-bounded).

Usage:
  python3 scripts/make-gbnf.py [artifact.json] [-o out.gbnf]
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

DEFAULT_ARTIFACT = Path(__file__).resolve().parent.parent / \
    "bench/constrain-masks-probed.json"

# Class tokens consume via character-class rules, not literals.
CLASS_RULES = {
    "ident":  r"[a-z] [a-z0-9-]*",
    "number": r"[0-9]+ (\".\" [0-9]+)?",
}

# canonical string rule transplanted from llama.cpp grammars/json.gbnf
STRING_RULE_RAW = (
    r'string ::= "\"" ( [^"\\] | "\\" (["\\bfnrt] | "u" [0-9a-fA-F]{4}) )* "\"" ws'
)

_REPL = {"\\": "bsh", " ": "sp", '"': "q", "[": "lb", "]": "rb", "{": "lc", "}": "rc",
         "(": "lp", ")": "rp", "|": "pipe", "*": "star", "/": "div",
         "+": "plus", "-": "minus", "=": "eq", "<": "lt", ">": "gt",
         "?": "q2", "!": "ex", "&": "amp", "^": "car", "~": "tl",
         "@": "at", "$": "dollar", "_": "us", ":": "colon", ";": "semi", ".": "dot",
         ",": "comma", "%": "pct"}


def safe(tok: str) -> str:
    return "t-" + "".join(_REPL.get(ch, ch) for ch in tok)


def lit(tok: str) -> str:
    return '"' + tok.replace("\\", "\\\\").replace('"', '\\"') + '"'


def shift(key: str, tok: str) -> str:
    """Successor context after consuming tok: (prev2 prev1) -> (prev1 tok)."""
    parts = key.split(" ")
    if parts[0] == "<START>":
        return parts[1] + " " + tok
    return parts[1] + " " + tok


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("artifact", nargs="?", default=str(DEFAULT_ARTIFACT))
    ap.add_argument("-o", "--out", default=None)
    args = ap.parse_args()

    d = json.loads(Path(args.artifact).read_text())
    if d.get("schemaVersion") != 2 or d.get("kind") != "probed-context-masks":
        print("error: expected a schemaVersion 2 probed-context artifact",
              file=sys.stderr)
        return 2
    contexts = d["contexts"]

    # discover states reachable from <START> via BFS; collect every state's
    # alternatives (already exhaustive per context from the probe)
    lines: list[str] = [
        "# ilo grammar — generated from probed transitions",
        "# (G3, schemaVersion 2 probed context masks)",
        f"# source artifact: {args.artifact}",
        "# states = probed two-token contexts; alternatives = oracle-allowed",
        "# tokens. Conservative: unobserved-but-valid continuations absent.",
        "",
        "root ::= ws start",
    ]

    starts = sorted(k for k in contexts if k.startswith("<START> "))
    if not starts:
        print("error: no <START> contexts in artifact", file=sys.stderr)
        return 2

    def sname(key: str) -> str:
        return "state-" + safe(key)

    starts_alt = " | ".join(f"ws {sname(s)}" for s in starts)
    lines.append(f"start ::= {starts_alt}")

    emitted: list[str] = []
    seen: set[str] = set()
    todo = sorted(starts)
    while todo:
        key = todo.pop(0)
        if key in seen:
            continue
        seen.add(key)
        # "<EOF>" marks a valid program end at this context: an empty
        # alternative (GBNF epsilon). Not a state transition.
        toks = [t for t in sorted(contexts.get(key, [])) if t != "<EOF>"]
        ends = "<EOF>" in contexts.get(key, [])
        st = sname(key)
        alts = []
        if ends:
            alts.append('""')
        if toks:
            for t in toks:
                nxt = shift(key, t)
                if t in CLASS_RULES:
                    alts.append(f"{t} ws {sname(nxt)}")
                else:
                    alts.append(f"{lit(t)} ws {sname(nxt)}")
                todo.append(nxt)
        if not alts:
            # dead end: probe-only continuation, no corpus successor.
            lines.append(f"{st} ::= \"\"")
            continue
        lines.append(f"{st} ::= " + " | ".join(alts))
        emitted.append(st)

    for cls, rule in CLASS_RULES.items():
        lines.append(f"{cls} ::= " + rule.replace('\\"', '"'))
    lines.append(STRING_RULE_RAW)

    lines.append('ws ::= [ \\n\\t]*')

    out = Path(args.out) if args.out else Path("constrain/ilo.gbnf")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text("\n".join(lines) + "\n")
    print(f"gbnf: {len(seen)} states, {len(lines)} lines -> {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
