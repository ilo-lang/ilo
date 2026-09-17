#!/usr/bin/env python3
"""eval_ilo_finetune.py — Phase 4 evaluation (PLAN.md Phase 4).

Question: does LoRA fine-tuning on the ilo corpus let a 0.5B model produce
ilo-check-valid programs WITHOUT the curated spec in context — i.e. did
fine-tuning delete the spec-loading term?

Arms on the same val tasks (greedy decode):
  1. ft-nospec  — base + adapter, system = "<<ILO_SPEC_INTERNALIZED>>"
  2. ft-spec    — base + adapter, system = full curated spec (sanity arm)
  3. base-spec  — pure base model, system = full curated spec (G2 path)

Metric: ilo check exit 0. Results JSON: bench/finetune/eval-results.json.

Usage:
  /tmp/ft-env/bin/python scripts/eval_ilo_finetune.py [--limit 35]
"""

from __future__ import annotations

import argparse
import json
import sys
import subprocess
import tempfile
from pathlib import Path

import torch
from peft import PeftModel
from transformers import AutoModelForCausalLM, AutoTokenizer

ROOT = Path(__file__).resolve().parent.parent
VAL = ROOT / "bench/finetune/val.jsonl"
CURATED = ["ilo-language", "ilo-builtins-core", "ilo-builtins-sig"]
SPEC_TAG = "<<ILO_SPEC_INTERNALIZED>>"


def load_spec() -> str:
    parts = []
    for m in CURATED:
        for base in (ROOT / "skills/ilo", ROOT / "docs/reference"):
            p = base / f"{m}.md"
            if p.exists():
                parts.append(p.read_text())
                break
    return "\n\n".join(parts)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--adapter",
                    default=str(ROOT / "bench/finetune/adapter"))
    ap.add_argument("--base", default="Qwen/Qwen2.5-0.5B-Instruct")
    ap.add_argument("--limit", type=int, default=35)
    ap.add_argument("--ilo", default=str(ROOT / "target/release/ilo"))
    ap.add_argument("--out",
                    default=str(ROOT / "bench/finetune/eval-results.json"))
    args = ap.parse_args()

    val = [json.loads(l) for l in VAL.read_text().splitlines() if l.strip()]
    val = val[: args.limit]

    tok = AutoTokenizer.from_pretrained(args.base)
    if tok.pad_token is None:
        tok.pad_token = tok.eos_token

    spec = load_spec()

    def check(src: str) -> dict:
        """Returns {'parse_ok': bool, 'full_ok': bool} from ilo check JSON."""
        with tempfile.NamedTemporaryFile("w", suffix=".ilo",
                                         delete=False) as f:
            f.write(src)
            path = f.name
        r = subprocess.run([args.ilo, "check", path, "--json"],
                           capture_output=True, text=True)
        Path(path).unlink(missing_ok=True)
        diags = []
        try:
            diags = json.loads(r.stdout)
            if isinstance(diags, dict):
                diags = diags.get("diagnostics", [])
        except json.JSONDecodeError:
            pass
        if r.returncode == 0 and not diags:
            return {"parse_ok": True, "full_ok": True}
        parse_bad = [d for d in diags
                     if str(d.get("code", "")).startswith("ILO-P")]
        lex_bad = [d for d in diags
                   if str(d.get("code", "")).startswith("ILO-L")]
        return {"parse_ok": not (parse_bad or lex_bad),
                "full_ok": r.returncode == 0 and not diags}

    def generate(model, system: str, task: str) -> str:
        msgs = [{"role": "system", "content": system},
                {"role": "user", "content": task}]
        text = tok.apply_chat_template(msgs, tokenize=False,
                                       add_generation_prompt=True)
        ids = tok(text, return_tensors="pt", add_special_tokens=False)
        with torch.no_grad():
            out = model.generate(**ids, max_new_tokens=256, do_sample=False,
                                 pad_token_id=tok.eos_token_id)
        return tok.decode(out[0][ids["input_ids"].shape[1]:],
                          skip_special_tokens=True)

    model = AutoModelForCausalLM.from_pretrained(
        args.base, torch_dtype=torch.float32, low_cpu_mem_usage=True)
    model.eval()

    results: dict[str, dict] = {}
    tasks = [(r["messages"][-2]["content"], i) for i, r in enumerate(val)]

    # ── arm 1+2: merged adapter model ────────────────────────────────────
    from peft import PeftModel
    merged = PeftModel.from_pretrained(
        AutoModelForCausalLM.from_pretrained(
            args.base, torch_dtype=torch.float32, low_cpu_mem_usage=True),
        args.adapter)
    merged = merged.merge_and_unload()
    merged.eval()
    print("adapter merged", file=sys.stderr)

    for arm, sysprompt in (("ft-nospec", SPEC_TAG), ("ft-spec", spec)):
        ok = 0
        rows = []
        for task, i in tasks:
            src = generate(merged, sysprompt, task)
            c = check(src)
            ok += c["full_ok"]
            rows.append({"i": i, "src": src, "check": c})
            print(f"  [{arm} {i + 1}/{len(tasks)}] "
                  f"{'OK' if c['full_ok'] else 'fail'}", file=sys.stderr)
        parse_ok = sum(1 for r in rows if r["check"]["parse_ok"])
        full_ok = sum(1 for r in rows if r["check"]["full_ok"])
        results[arm] = {"parse_ok": parse_ok, "full_ok": full_ok,
                        "n": len(tasks), "rows": rows}
        print(f"{arm}: parse {parse_ok}/{len(tasks)}, "
              f"full {full_ok}/{len(tasks)}", file=sys.stderr)
    del merged

    # ── arm 3: pure base + curated spec (the G2 agent path) ─────────────
    ok = 0
    rows = []
    for task, i in tasks:
        src = generate(model, spec, task)
        c = check(src)
        ok += c["full_ok"]
        rows.append({"i": i, "src": src, "check": c})
        print(f"  [base-spec {i + 1}/{len(tasks)}] "
              f"{'OK' if c['full_ok'] else 'fail'}", file=sys.stderr)
    parse_ok = sum(1 for r in rows if r["check"]["parse_ok"])
    full_ok = sum(1 for r in rows if r["check"]["full_ok"])
    results["base-spec"] = {"parse_ok": parse_ok, "full_ok": full_ok,
                            "n": len(tasks), "rows": rows}
    print(f"base-spec: {ok}/{len(tasks)} check-valid", file=sys.stderr)

    summary = {}
    for arm in ("ft-nospec", "ft-spec", "base-spec"):
        r = results[arm]
        summary[arm] = {"parse_ok": r["parse_ok"], "full_ok": r["full_ok"],
                        "n": r["n"]}
    out = {"schemaVersion": 1, "base": args.base, "adapter": args.adapter,
           "val_n": len(val), "summary": summary, "results": results}
    Path(args.out).write_text(json.dumps(out, indent=2, default=str))
    print(json.dumps(summary, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
