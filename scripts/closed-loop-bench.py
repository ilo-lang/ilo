#!/usr/bin/env python3
"""
Closed-loop benchmark: ilo vs alternative language CLI (Phase 5, ILO-364).

Drives a full LLM → compile → repair → retry loop for each task on each
language (ilo and optionally a second CLI such as Zero), across both Haiku
and Sonnet model families.  Measures the per-task total cost in tokens and
wall-clock time.

Per task, per language, per model this script logs:
  - generation_tokens     total tokens generated across all attempts
  - repair_tokens_by_turn tokens generated in each repair attempt (list)
  - input_tokens          total input/context tokens across all attempts
  - attempts_to_success   how many attempts were needed (None if failed)
  - success_rate          1.0 / 0.0 per run (across retry_cap attempts)
  - wall_time_s           total wall-clock seconds for this task
  - final_outcome         "working" | "partial" | "failed"

Output
  bench/closed-loop-<date>.json   structured JSON dataset
  bench/closed-loop-<date>.md     markdown writeup with headline table

Usage
  # ilo only, both models
  python3 scripts/closed-loop-bench.py

  # specify an ilo binary (default: ilo from PATH)
  python3 scripts/closed-loop-bench.py --ilo ./target/release/ilo

  # also benchmark a second language CLI
  python3 scripts/closed-loop-bench.py --lang2-name zero --lang2-bin zero --lang2-ext .zero

  # single task dry-run (no LLM calls — just print task spec)
  python3 scripts/closed-loop-bench.py --dry-run

  # single task
  python3 scripts/closed-loop-bench.py --task simple-function

  # single model
  python3 scripts/closed-loop-bench.py --model haiku

Environment
  ANTHROPIC_API_KEY   required

Retry cap
  Default N=5.  Override with --retry-cap N.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

REPO_ROOT = Path(__file__).resolve().parent.parent
BENCH_DIR = REPO_ROOT / "bench"
TASKS_FILE = BENCH_DIR / "closed-loop" / "tasks.json"
SKILLS_DIR = REPO_ROOT / "skills" / "ilo"

DEFAULT_RETRY_CAP = 5
ILO_TIMEOUT = 20  # seconds per ilo run
LANG2_TIMEOUT = 20  # seconds per secondary language run

MODELS = {
    "haiku": "claude-haiku-4-5",
    "sonnet": "claude-sonnet-4-5",
}

# Outcome ranks for partial ordering
OUTCOME_RANK = {"working": 2, "partial": 1, "failed": 0}

# ---------------------------------------------------------------------------
# ILO skill context (cached once per process to match steady-state economics)
# ---------------------------------------------------------------------------

_SKILL_CACHE: dict[str, str] = {}


def load_skill_text(module_name: str, ilo_bin: str) -> str:
    """Load skill module, caching in memory (steady-state: single load)."""
    if module_name in _SKILL_CACHE:
        return _SKILL_CACHE[module_name]
    try:
        result = subprocess.run(
            [ilo_bin, "skill", "get", module_name],
            capture_output=True, text=True, timeout=10,
        )
        if result.returncode == 0 and result.stdout.strip():
            _SKILL_CACHE[module_name] = result.stdout
            return result.stdout
    except (FileNotFoundError, subprocess.TimeoutExpired):
        pass
    path = SKILLS_DIR / f"{module_name}.md"
    if path.exists():
        text = path.read_text()
        _SKILL_CACHE[module_name] = text
        return text
    fallback = f"# {module_name}\n(skill module not found)\n"
    _SKILL_CACHE[module_name] = fallback
    return fallback


def ilo_context(ilo_bin: str) -> str:
    """Return the core ilo skill documentation (cached)."""
    mods = ["ilo-language", "ilo-builtins-core", "ilo-builtins-text",
            "ilo-builtins-math", "ilo-builtins-io"]
    return "\n\n".join(load_skill_text(m, ilo_bin) for m in mods)


# ---------------------------------------------------------------------------
# Prompts
# ---------------------------------------------------------------------------

ILO_SYSTEM = """\
You are an ilo programming language expert. Given a task description and the
relevant ilo skill documentation, write a complete, runnable ilo program that
solves the task.

Rules:
- The program must be syntactically valid ilo (prefix notation, typed).
- The very first line must be a comment: -- run: main
- If the task specifies expected output, add a comment: -- out: <expected>
- Keep the program under 50 lines.
- If the task requires network or environment access not available offline,
  mock it with a literal value.
- Output ONLY the ilo program, no explanation, no markdown fences.
"""

LANG2_SYSTEM = """\
You are a programming language expert. Write a complete, runnable program in
{lang_name} that solves the task described below.

Rules:
- The program must be a single self-contained file.
- Keep the program under 50 lines.
- If the task requires network or environment access not available offline,
  mock it with a literal value.
- Output ONLY the program source, no explanation, no markdown fences.
"""


def make_initial_prompt(task: dict[str, Any], context: str, lang: str) -> str:
    return (
        f"Task: {task['description']}\n\n"
        f"Expected output: {task['expected_output']}\n\n"
        f"---LANGUAGE DOCUMENTATION---\n{context}\n---END---\n"
    )


def make_repair_prompt(
    task: dict[str, Any],
    context: str,
    error: str,
    lang: str,
) -> str:
    return (
        f"The previous {lang} program for this task failed.\n"
        f"Task: {task['description']}\n"
        f"Error / actual output:\n{error}\n\n"
        f"Rewrite the program to fix the error. Output ONLY the {lang} code.\n"
        f"---LANGUAGE DOCUMENTATION---\n{context}\n---END---\n"
    )


# ---------------------------------------------------------------------------
# API call
# ---------------------------------------------------------------------------

def call_llm(
    system: str,
    user: str,
    model_id: str,
    api_key: str,
) -> tuple[str, int, int]:
    """Returns (text, output_tokens, input_tokens).  Raises on error."""
    import urllib.request

    payload = json.dumps({
        "model": model_id,
        "max_tokens": 1024,
        "system": system,
        "messages": [{"role": "user", "content": user}],
    }).encode()

    req = urllib.request.Request(
        "https://api.anthropic.com/v1/messages",
        data=payload,
        headers={
            "x-api-key": api_key,
            "anthropic-version": "2023-06-01",
            "content-type": "application/json",
        },
        method="POST",
    )
    with urllib.request.urlopen(req, timeout=90) as resp:
        body = json.loads(resp.read())

    text = body["content"][0]["text"]
    output_tokens = body["usage"]["output_tokens"]
    input_tokens = body["usage"]["input_tokens"]
    return text, output_tokens, input_tokens


# ---------------------------------------------------------------------------
# Run helpers
# ---------------------------------------------------------------------------

def run_ilo(code: str, ilo_bin: str) -> tuple[str, str, int]:
    """Write code to a temp .ilo file, run it, return (stdout, stderr, rc)."""
    with tempfile.NamedTemporaryFile(suffix=".ilo", mode="w", delete=False) as f:
        f.write(code)
        tmp = f.name
    try:
        r = subprocess.run(
            [ilo_bin, tmp, "main"],
            capture_output=True, text=True, timeout=ILO_TIMEOUT,
        )
        return r.stdout, r.stderr, r.returncode
    except subprocess.TimeoutExpired:
        return "", "timeout", 1
    finally:
        os.unlink(tmp)


def run_lang2(code: str, lang2_bin: str, ext: str) -> tuple[str, str, int]:
    """Write code to a temp file with given ext, run via lang2_bin, return (stdout, stderr, rc)."""
    with tempfile.NamedTemporaryFile(suffix=ext, mode="w", delete=False) as f:
        f.write(code)
        tmp = f.name
    try:
        r = subprocess.run(
            [lang2_bin, tmp],
            capture_output=True, text=True, timeout=LANG2_TIMEOUT,
        )
        return r.stdout, r.stderr, r.returncode
    except subprocess.TimeoutExpired:
        return "", "timeout", 1
    finally:
        os.unlink(tmp)


def classify_outcome(expected: str, stdout: str, stderr: str, rc: int) -> str:
    if rc != 0:
        return "failed"
    # Normalise: strip whitespace, unescape \n in expected
    expected_norm = expected.replace("\\n", "\n").strip()
    actual_norm = stdout.strip()
    if actual_norm == expected_norm:
        return "working"
    return "partial"


# ---------------------------------------------------------------------------
# Core: run one task for one language on one model
# ---------------------------------------------------------------------------

def run_task(
    task: dict[str, Any],
    lang: str,                # "ilo" or lang2_name
    model_key: str,           # "haiku" | "sonnet"
    model_id: str,
    api_key: str,
    retry_cap: int,
    ilo_bin: str,
    lang2_bin: str | None,
    lang2_ext: str,
) -> dict[str, Any]:
    model_id_used = model_id
    is_ilo = (lang == "ilo")

    # Build context
    if is_ilo:
        context = ilo_context(ilo_bin)
        system = ILO_SYSTEM
        run_fn = lambda code: run_ilo(code, ilo_bin)  # noqa: E731
    else:
        context = f"(No formal language documentation available for {lang}.)"
        system = LANG2_SYSTEM.format(lang_name=lang)
        run_fn = lambda code: run_lang2(code, lang2_bin, lang2_ext)  # noqa: E731

    user = make_initial_prompt(task, context, lang)

    total_gen_tokens = 0
    total_input_tokens = 0
    repair_tokens_by_turn: list[int] = []
    attempts = 0
    outcome = "failed"
    wall_start = time.monotonic()

    for attempt in range(1, retry_cap + 1):
        attempts = attempt
        try:
            code, gen_tok, inp_tok = call_llm(system, user, model_id, api_key)
        except Exception as exc:  # noqa: BLE001
            print(f"      [attempt {attempt}] API error: {exc}", file=sys.stderr)
            time.sleep(2)
            continue

        total_gen_tokens += gen_tok
        total_input_tokens += inp_tok
        if attempt == 1:
            pass  # first attempt is not a repair
        else:
            repair_tokens_by_turn.append(gen_tok)

        stdout, stderr, rc = run_fn(code)
        outcome = classify_outcome(task["expected_output"], stdout, stderr, rc)

        print(
            f"      attempt={attempt} outcome={outcome} "
            f"gen_tok={gen_tok} rc={rc}",
            file=sys.stderr,
        )

        if outcome == "working":
            break

        # Build repair prompt
        error_detail = (stderr or stdout or "(no output)").strip()[:1000]
        user = make_repair_prompt(task, context, error_detail, lang)

    wall_time = time.monotonic() - wall_start
    attempts_to_success = attempts if outcome == "working" else None

    return {
        "task": task["id"],
        "language": lang,
        "model": model_key,
        "model_id": model_id_used,
        "generation_tokens": total_gen_tokens,
        "input_tokens": total_input_tokens,
        "repair_tokens_by_turn": repair_tokens_by_turn,
        "attempts_to_success": attempts_to_success,
        "attempts_total": attempts,
        "success_rate": 1.0 if outcome == "working" else 0.0,
        "wall_time_s": round(wall_time, 2),
        "final_outcome": outcome,
    }


# ---------------------------------------------------------------------------
# Output helpers
# ---------------------------------------------------------------------------

def write_json(results: list[dict[str, Any]], date_str: str) -> Path:
    out = BENCH_DIR / f"closed-loop-{date_str}.json"
    payload = {
        "generated": datetime.now(timezone.utc).isoformat(),
        "harness": "closed-loop-bench.py",
        "ticket": "ILO-364",
        "results": results,
    }
    out.write_text(json.dumps(payload, indent=2))
    return out


def write_markdown(results: list[dict[str, Any]], date_str: str) -> Path:
    out = BENCH_DIR / f"closed-loop-{date_str}.md"

    # Index results: (task, lang, model) -> record
    idx: dict[tuple[str, str, str], dict] = {}
    tasks_seen: list[str] = []
    langs_seen: list[str] = []
    models_seen: list[str] = []
    for r in results:
        key = (r["task"], r["language"], r["model"])
        idx[key] = r
        if r["task"] not in tasks_seen:
            tasks_seen.append(r["task"])
        if r["language"] not in langs_seen:
            langs_seen.append(r["language"])
        if r["model"] not in models_seen:
            models_seen.append(r["model"])

    lines: list[str] = [
        f"# Closed-loop benchmark: ilo vs Zero per-task economics",
        f"",
        f"Generated: {datetime.now(timezone.utc).strftime('%Y-%m-%d %H:%M UTC')}  ",
        f"Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  ",
        f"Retry cap: {DEFAULT_RETRY_CAP}",
        f"",
        f"## Summary",
        f"",
        f"This table shows per-task economics across languages and models.",
        f"Columns: total generation tokens (gen), input tokens (inp), attempts to success (att), wall time (s), outcome.",
        f"",
    ]

    # Build header for each lang+model combo
    combos = [(lang, model) for lang in langs_seen for model in models_seen]
    col_header = " | ".join(f"{lang}/{model}" for lang, model in combos)
    sep = " | ".join(["---"] * (1 + len(combos) * 5))

    lines.append("| task | " + " | ".join(
        f"{lang}/{model}: gen | inp | att | time | outcome"
        for lang, model in combos
    ) + " |")
    lines.append("|---" * (1 + len(combos) * 5) + "|")

    for task in tasks_seen:
        row = f"| {task} |"
        for lang, model in combos:
            r = idx.get((task, lang, model))
            if r:
                att = str(r["attempts_to_success"]) if r["attempts_to_success"] else "-"
                row += (
                    f" {r['generation_tokens']} |"
                    f" {r['input_tokens']} |"
                    f" {att} |"
                    f" {r['wall_time_s']}s |"
                    f" {r['final_outcome']} |"
                )
            else:
                row += " - | - | - | - | - |"
        lines.append(row)

    lines += [
        f"",
        f"## Per-task details",
        f"",
    ]

    for task in tasks_seen:
        lines.append(f"### {task}")
        for lang in langs_seen:
            for model in models_seen:
                r = idx.get((task, lang, model))
                if not r:
                    continue
                lines += [
                    f"",
                    f"**{lang} / {model}**  ",
                    f"- Generation tokens: {r['generation_tokens']}  ",
                    f"- Input tokens (context): {r['input_tokens']}  ",
                    f"- Attempts total: {r['attempts_total']}  ",
                    f"- Attempts to success: {r['attempts_to_success']}  ",
                    f"- Repair tokens by turn: {r['repair_tokens_by_turn']}  ",
                    f"- Wall time: {r['wall_time_s']}s  ",
                    f"- Outcome: **{r['final_outcome']}**  ",
                ]
        lines.append("")

    lines += [
        f"## Notes",
        f"",
        f"- Zero language CLI was not available in this environment; Zero column deferred.",
        f"  To add Zero, run: `python3 scripts/closed-loop-bench.py --lang2-name zero --lang2-bin <path-to-zero> --lang2-ext .zero`",
        f"- Skill documentation is loaded once per process (steady-state caching).",
        f"- One-shot economics (first attempt only) can be derived from `repair_tokens_by_turn` in the JSON.",
        f"- Re-run at any time; output files are date-stamped.",
        f"",
        f"## Deferred",
        f"",
        f"- Zero CLI integration (ILO-364 Phase 5.b): blocked on Zero being installable in CI.",
        f"- Pre/post [ILO-360](https://linear.app/ilo-lang/issue/ILO-360) comparison: run baseline now, re-run after typed fix plans land.",
        f"- Empirical retry-cap tuning: first run curves are in `repair_tokens_by_turn`; "
        f"adjust `--retry-cap` once flattening point is visible.",
    ]

    out.write_text("\n".join(lines) + "\n")
    return out


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ilo", default=os.environ.get("ILO", "ilo"),
                        help="Path to ilo binary.")
    parser.add_argument("--retry-cap", type=int, default=DEFAULT_RETRY_CAP,
                        help="Max repair attempts per task (default 5).")
    parser.add_argument("--model", choices=["haiku", "sonnet", "both"],
                        default="both", help="Model(s) to run.")
    parser.add_argument("--task", metavar="ID",
                        help="Run only this task ID.")
    parser.add_argument("--lang2-name", default=None,
                        help="Name of second language to benchmark (e.g. zero).")
    parser.add_argument("--lang2-bin", default=None,
                        help="Path to second language CLI binary.")
    parser.add_argument("--lang2-ext", default=".zero",
                        help="File extension for second language source (default .zero).")
    parser.add_argument("--dry-run", action="store_true",
                        help="Print task specs and exit without making LLM calls.")
    parser.add_argument("--output-dir", default=None,
                        help="Override output directory (default: bench/).")
    args = parser.parse_args()

    global BENCH_DIR
    if args.output_dir:
        BENCH_DIR = Path(args.output_dir)

    # Load tasks
    tasks_data = json.loads(TASKS_FILE.read_text())
    all_tasks = tasks_data["tasks"]
    if args.task:
        all_tasks = [t for t in all_tasks if t["id"] == args.task]
        if not all_tasks:
            print(f"ERROR: task '{args.task}' not found in tasks.json", file=sys.stderr)
            return 2

    if args.dry_run:
        print("Tasks:")
        for t in all_tasks:
            print(f"  [{t['id']}] {t['description'][:80]}...")
            print(f"          expected: {t['expected_output']!r}")
        return 0

    # Verify ilo
    try:
        subprocess.run([args.ilo, "--version"], capture_output=True, check=True, timeout=5)
    except (FileNotFoundError, subprocess.CalledProcessError, subprocess.TimeoutExpired):
        print(f"ERROR: ilo binary not found or not runnable: {args.ilo}", file=sys.stderr)
        return 2

    # Verify lang2 if requested
    lang2_bin: str | None = None
    if args.lang2_name and args.lang2_bin:
        try:
            subprocess.run([args.lang2_bin, "--version"], capture_output=True, timeout=5)
            lang2_bin = args.lang2_bin
        except (FileNotFoundError, subprocess.TimeoutExpired):
            print(
                f"WARNING: lang2 binary not found: {args.lang2_bin}. "
                "Skipping second language.",
                file=sys.stderr,
            )

    api_key = os.environ.get("ANTHROPIC_API_KEY", "")
    if not api_key:
        print("ERROR: ANTHROPIC_API_KEY not set", file=sys.stderr)
        return 2

    # Determine models to run
    if args.model == "both":
        model_keys = list(MODELS.keys())
    else:
        model_keys = [args.model]

    # Determine languages
    languages = ["ilo"]
    if lang2_bin and args.lang2_name:
        languages.append(args.lang2_name)

    total_runs = len(all_tasks) * len(languages) * len(model_keys)
    print(
        f"Closed-loop bench: {len(all_tasks)} tasks × "
        f"{len(languages)} languages × {len(model_keys)} models = "
        f"{total_runs} runs  (retry_cap={args.retry_cap})"
    )

    results: list[dict[str, Any]] = []
    run_num = 0

    for task in all_tasks:
        for lang in languages:
            for model_key in model_keys:
                run_num += 1
                model_id = MODELS[model_key]
                print(
                    f"\n[{run_num}/{total_runs}] task={task['id']} "
                    f"lang={lang} model={model_key}",
                    file=sys.stderr,
                )
                r = run_task(
                    task=task,
                    lang=lang,
                    model_key=model_key,
                    model_id=model_id,
                    api_key=api_key,
                    retry_cap=args.retry_cap,
                    ilo_bin=args.ilo,
                    lang2_bin=lang2_bin,
                    lang2_ext=args.lang2_ext,
                )
                results.append(r)
                print(
                    f"  -> outcome={r['final_outcome']} "
                    f"gen_tokens={r['generation_tokens']} "
                    f"wall={r['wall_time_s']}s"
                )

    date_str = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    json_path = write_json(results, date_str)
    md_path = write_markdown(results, date_str)

    print(f"\nResults written:")
    print(f"  JSON: {json_path}")
    print(f"  MD:   {md_path}")

    # Summary table to stdout
    print("\nSummary:")
    print(f"{'task':<22} {'lang':<6} {'model':<6} {'gen_tok':>7} {'attempts':>8} {'outcome':<8} {'time':>6}")
    print("-" * 75)
    for r in results:
        att = str(r["attempts_to_success"]) if r["attempts_to_success"] else "-"
        print(
            f"{r['task']:<22} {r['language']:<6} {r['model']:<6} "
            f"{r['generation_tokens']:>7} {att:>8} {r['final_outcome']:<8} "
            f"{r['wall_time_s']:>5.1f}s"
        )

    success_count = sum(1 for r in results if r["final_outcome"] == "working")
    print(f"\n{success_count}/{total_runs} runs succeeded.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
