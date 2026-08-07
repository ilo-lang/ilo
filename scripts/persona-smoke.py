#!/usr/bin/env python3
"""
Persona-corpus smoke harness for skill-module regression (ILO-384).

For each persona in bench/persona-smoke.txt, ask Claude Haiku to write an
ilo program that solves a representative task for that persona (using the
skill modules appropriate for the persona block).  Run the generated program
with `ilo`, record outcome / generation-tokens / attempts / tool-use-count,
and compare against a stored JSON baseline.

Exit codes
  0  all personas pass regression gate
  1  at least one persona regressed (outcome or token delta > threshold)
  2  usage / setup error

Usage
  # record a fresh baseline (writes bench/persona-smoke-baseline.json)
  python3 scripts/persona-smoke.py --baseline

  # compare current skill spec against the stored baseline (CI mode)
  python3 scripts/persona-smoke.py

  # show per-module token sizes and exit (no LLM calls)
  python3 scripts/persona-smoke.py --token-report

Environment
  ANTHROPIC_API_KEY   required for --baseline and default (comparison) mode
  ILO                 path to ilo binary (default: ilo from PATH)

Regression thresholds
  outcome:  any working -> partial or failed is a hard fail
  tokens:   mean generation tokens may not increase by more than TOKEN_REGRESS_PCT %
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
from pathlib import Path
from typing import Any

import tiktoken

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

REPO_ROOT = Path(__file__).resolve().parent.parent
SKILLS_DIR = REPO_ROOT / "skills" / "ilo"
SMOKE_LIST = REPO_ROOT / "bench" / "persona-smoke.txt"
BASELINE_FILE = REPO_ROOT / "bench" / "persona-smoke-baseline.json"

TOKEN_REGRESS_PCT = 20      # mean generation-token increase threshold (%)
MAX_ATTEMPTS = 3            # repair attempts per persona before giving up
ILO_TIMEOUT = 20            # seconds per ilo run

# Outcome ranks: higher is better.  Regression = current rank < baseline rank.
OUTCOME_RANK = {"working": 2, "partial": 1, "failed": 0}

# Skill modules relevant to each dogfood block keyword.
BLOCK_SKILLS: dict[str, list[str]] = {
    "date":       ["ilo-language", "ilo-builtins-text"],
    "time":       ["ilo-language", "ilo-builtins-text"],
    "schedule":   ["ilo-language", "ilo-builtins-text"],
    "event":      ["ilo-language", "ilo-builtins-text"],
    "crypto":     ["ilo-language", "ilo-builtins-core", "ilo-builtins-math"],
    "http":       ["ilo-language", "ilo-builtins-io"],
    "api":        ["ilo-language", "ilo-builtins-io"],
    "batch":      ["ilo-language", "ilo-builtins-io"],
    "fetch":      ["ilo-language", "ilo-builtins-io"],
    "numeric":    ["ilo-language", "ilo-builtins-math"],
    "regression": ["ilo-language", "ilo-builtins-math"],
    "linear":     ["ilo-language", "ilo-builtins-math"],
    "k-means":    ["ilo-language", "ilo-builtins-math"],
    "kmeans":     ["ilo-language", "ilo-builtins-math"],
    "io":         ["ilo-language", "ilo-builtins-io"],
    "csv":        ["ilo-language", "ilo-builtins-io", "ilo-builtins-text"],
    "log":        ["ilo-language", "ilo-builtins-io", "ilo-builtins-text"],
    "record":     ["ilo-language", "ilo-language-records"],
    "config":     ["ilo-language", "ilo-language-records", "ilo-builtins-io"],
    "text":       ["ilo-language", "ilo-builtins-text"],
    "cron":       ["ilo-language", "ilo-builtins-text"],
    "mining":     ["ilo-language", "ilo-builtins-text"],
    "tool":       ["ilo-language", "ilo-tools", "ilo-agent"],
    "doc":        ["ilo-language", "ilo-tools", "ilo-agent"],
    "discovery":  ["ilo-language", "ilo-tools", "ilo-agent"],
}

DEFAULT_SKILLS = ["ilo-language", "ilo-builtins-core"]


# ---------------------------------------------------------------------------
# Helper: choose skill modules for a persona slug
# ---------------------------------------------------------------------------

def modules_for_persona(slug: str) -> list[str]:
    slug_lower = slug.lower()
    chosen: list[str] = []
    seen: set[str] = set()
    for keyword, mods in BLOCK_SKILLS.items():
        if keyword in slug_lower:
            for m in mods:
                if m not in seen:
                    chosen.append(m)
                    seen.add(m)
    if not chosen:
        chosen = list(DEFAULT_SKILLS)
    return chosen


# ---------------------------------------------------------------------------
# Helper: load skill module text from the installed binary or files
# ---------------------------------------------------------------------------

def load_skill_text(module_name: str, ilo_bin: str) -> str:
    """Load skill module content preferring the installed binary, fall back to file."""
    try:
        result = subprocess.run(
            [ilo_bin, "skill", "get", module_name],
            capture_output=True, text=True, timeout=10,
        )
        if result.returncode == 0 and result.stdout.strip():
            return result.stdout
    except (FileNotFoundError, subprocess.TimeoutExpired):
        pass
    # Fallback: read from repo
    path = SKILLS_DIR / f"{module_name}.md"
    if path.exists():
        return path.read_text()
    return f"# {module_name}\n(skill module not found)\n"


# ---------------------------------------------------------------------------
# Helper: token count per module
# ---------------------------------------------------------------------------

def module_token_report() -> dict[str, int]:
    enc = tiktoken.get_encoding("cl100k_base")
    report: dict[str, int] = {}
    for path in sorted(SKILLS_DIR.glob("ilo-*.md")):
        tokens = len(enc.encode(path.read_text()))
        report[path.stem] = tokens
    return report


def print_token_report() -> None:
    report = module_token_report()
    total = sum(report.values())
    print("\nSkill-module token sizes (cl100k_base):")
    for name, tokens in report.items():
        print(f"  {name:<26} {tokens:5d}")
    print(f"  {'TOTAL':<26} {total:5d}")
    print()


# ---------------------------------------------------------------------------
# Helper: build the system + user prompt for a persona
# ---------------------------------------------------------------------------

SYSTEM_PROMPT = """\
You are an ilo programming language expert.  Given a persona description and
the relevant ilo skill documentation, write a complete, runnable ilo program
that demonstrates the core task of that persona.

Rules:
- The program must be syntactically valid ilo (prefix notation, typed, no
  external imports beyond what ilo-tools declares).
- Include a `-- run: main` and `-- out: <expected_output>` header comment so
  the CI harness can verify correctness.
- Keep the program under 40 lines.  Prefer builtins over hand-rolled loops.
- If the task requires HTTP or tool calls, mock the external call with a
  literal value so the program runs offline without network access.
- Output ONLY the ilo program, no explanation, no markdown fences.
"""

def make_user_prompt(slug: str, skill_text: str) -> str:
    # Derive a human-readable task description from the slug.
    task = slug.replace("-", " ").replace("_", " ")
    return (
        f"Persona: {task}\n\n"
        f"Write a small ilo program that solves a representative task for this "
        f"persona.  The program should exercise the key builtins and patterns "
        f"described in the skill documentation below.\n\n"
        f"---SKILL DOCUMENTATION---\n{skill_text}\n---END---\n"
    )


# ---------------------------------------------------------------------------
# Helper: call Anthropic API (Haiku)
# ---------------------------------------------------------------------------

def call_haiku(system: str, user: str, api_key: str) -> tuple[str, int]:
    """Returns (generated_text, output_tokens).  Raises on API error."""
    import urllib.request

    payload = json.dumps({
        "model": "claude-haiku-4-5",
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
    with urllib.request.urlopen(req, timeout=60) as resp:
        body = json.loads(resp.read())

    text = body["content"][0]["text"]
    tokens = body["usage"]["output_tokens"]
    return text, tokens


# ---------------------------------------------------------------------------
# Helper: run generated ilo code and determine outcome
# ---------------------------------------------------------------------------

def run_ilo_code(code: str, ilo_bin: str) -> tuple[str, str, int]:
    """Write code to a temp file, run it, return (stdout, stderr, exit_code)."""
    with tempfile.NamedTemporaryFile(suffix=".ilo", mode="w", delete=False) as f:
        f.write(code)
        tmp_path = f.name
    try:
        result = subprocess.run(
            [ilo_bin, tmp_path, "main"],
            capture_output=True, text=True, timeout=ILO_TIMEOUT,
        )
        return result.stdout, result.stderr, result.returncode
    except subprocess.TimeoutExpired:
        return "", "timeout", 1
    finally:
        os.unlink(tmp_path)


def classify_outcome(code: str, stdout: str, stderr: str, exit_code: int) -> str:
    """Classify a run as 'working', 'partial', or 'failed'."""
    if exit_code != 0:
        return "failed"
    # Check declared expected output if present
    m = re.search(r"--\s*out:\s*(.+)", code)
    if m:
        expected = m.group(1).strip()
        actual = stdout.strip()
        if actual == expected:
            return "working"
        # Partial: program ran but output differs
        return "partial"
    # No expected output declared: running without error counts as working
    return "working"


# ---------------------------------------------------------------------------
# Core: run one persona and return metrics
# ---------------------------------------------------------------------------

def run_persona(
    slug: str, ilo_bin: str, api_key: str
) -> dict[str, Any]:
    modules = modules_for_persona(slug)
    skill_text = "\n\n".join(load_skill_text(m, ilo_bin) for m in modules)

    system = SYSTEM_PROMPT
    user = make_user_prompt(slug, skill_text)

    total_gen_tokens = 0
    attempts = 0
    outcome = "failed"
    tool_use_count = 0  # ilo programs don't call tools in smoke runs; reserved

    last_code = ""
    for attempt in range(1, MAX_ATTEMPTS + 1):
        attempts = attempt
        try:
            code, gen_tokens = call_haiku(system, user, api_key)
        except Exception as exc:  # noqa: BLE001
            print(f"    [attempt {attempt}] API error: {exc}", file=sys.stderr)
            time.sleep(2)
            continue

        total_gen_tokens += gen_tokens
        last_code = code

        stdout, stderr, exit_code = run_ilo_code(code, ilo_bin)
        outcome = classify_outcome(code, stdout, stderr, exit_code)

        if outcome == "working":
            break

        # Feed back the error for the next attempt
        user = (
            f"The previous ilo program for persona '{slug}' failed.\n"
            f"Error output:\n{stderr or stdout or '(no output)'}\n\n"
            f"Rewrite the program to fix the error.  Output ONLY the ilo code.\n"
            f"---SKILL DOCUMENTATION---\n{skill_text}\n---END---\n"
        )

    return {
        "persona": slug,
        "outcome": outcome,
        "generation_tokens": total_gen_tokens,
        "attempts": attempts,
        "tool_use_count": tool_use_count,
        "modules_loaded": modules,
    }


# ---------------------------------------------------------------------------
# Baseline record / compare
# ---------------------------------------------------------------------------

def record_baseline(results: list[dict[str, Any]]) -> None:
    baseline = {
        "generated": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "personas": {r["persona"]: r for r in results},
        "module_tokens": module_token_report(),
    }
    BASELINE_FILE.write_text(json.dumps(baseline, indent=2))
    print(f"\nBaseline written to {BASELINE_FILE}")


def compare_to_baseline(
    results: list[dict[str, Any]],
    baseline_data: dict[str, Any],
) -> tuple[bool, list[str]]:
    """Returns (passed, list_of_failure_messages)."""
    failures: list[str] = []
    baseline_personas = baseline_data.get("personas", {})

    baseline_tokens = [
        v["generation_tokens"]
        for v in baseline_personas.values()
        if v["generation_tokens"] > 0
    ]
    current_tokens = [r["generation_tokens"] for r in results if r["generation_tokens"] > 0]

    baseline_mean = sum(baseline_tokens) / len(baseline_tokens) if baseline_tokens else 0
    current_mean = sum(current_tokens) / len(current_tokens) if current_tokens else 0

    for result in results:
        slug = result["persona"]
        current_outcome = result["outcome"]
        baseline_entry = baseline_personas.get(slug)

        if baseline_entry is None:
            # New persona — no baseline to compare against, skip gate
            print(f"  {slug}: NEW (no baseline)")
            continue

        baseline_outcome = baseline_entry["outcome"]
        current_rank = OUTCOME_RANK.get(current_outcome, 0)
        baseline_rank = OUTCOME_RANK.get(baseline_outcome, 0)

        token_delta = result["generation_tokens"] - baseline_entry["generation_tokens"]
        token_pct = (
            100 * token_delta / baseline_entry["generation_tokens"]
            if baseline_entry["generation_tokens"] > 0
            else 0
        )

        status = "OK"
        if current_rank < baseline_rank:
            msg = (
                f"  FAIL {slug}: outcome regressed "
                f"{baseline_outcome} -> {current_outcome}"
            )
            failures.append(msg)
            status = "REGRESSED"
        print(
            f"  {slug:<28} {current_outcome:<8} "
            f"gen_tokens={result['generation_tokens']:4d} "
            f"(delta {token_delta:+d}, {token_pct:+.0f}%)  "
            f"attempts={result['attempts']}  {status}"
        )

    # Aggregate token check
    if baseline_mean > 0 and current_mean > 0:
        mean_pct = 100 * (current_mean - baseline_mean) / baseline_mean
        print(
            f"\n  Mean generation tokens: baseline={baseline_mean:.0f}  "
            f"current={current_mean:.0f}  delta={mean_pct:+.1f}%"
        )
        if mean_pct > TOKEN_REGRESS_PCT:
            msg = (
                f"  FAIL mean generation tokens increased by "
                f"{mean_pct:.1f}% (threshold {TOKEN_REGRESS_PCT}%)"
            )
            failures.append(msg)

    return len(failures) == 0, failures


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

def load_persona_list() -> list[str]:
    slugs: list[str] = []
    for line in SMOKE_LIST.read_text().splitlines():
        line = line.strip()
        if line and not line.startswith("#"):
            slugs.append(line)
    return slugs


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--baseline",
        action="store_true",
        help="Run all personas and write a fresh baseline JSON.",
    )
    parser.add_argument(
        "--token-report",
        action="store_true",
        help="Print per-module token sizes and exit (no LLM calls).",
    )
    parser.add_argument(
        "--ilo",
        default=os.environ.get("ILO", "ilo"),
        help="Path to the ilo binary (default: ilo from PATH).",
    )
    parser.add_argument(
        "--persona",
        metavar="SLUG",
        help="Run a single persona instead of the full smoke set.",
    )
    args = parser.parse_args()

    # Always print module token sizes as part of the CI summary.
    print_token_report()

    if args.token_report:
        return 0

    api_key = os.environ.get("ANTHROPIC_API_KEY", "")
    if not api_key:
        print("ERROR: ANTHROPIC_API_KEY not set", file=sys.stderr)
        return 2

    # Verify ilo binary
    ilo_bin = args.ilo
    try:
        subprocess.run([ilo_bin, "--version"], capture_output=True, check=True, timeout=5)
    except (FileNotFoundError, subprocess.CalledProcessError, subprocess.TimeoutExpired):
        print(f"ERROR: ilo binary not found or not runnable: {ilo_bin}", file=sys.stderr)
        print("Run: scripts/ensure-ilo.sh", file=sys.stderr)
        return 2

    slugs = [args.persona] if args.persona else load_persona_list()

    print(f"Running {len(slugs)} persona(s) on Haiku...\n")
    results: list[dict[str, Any]] = []
    for slug in slugs:
        print(f"  -> {slug}")
        r = run_persona(slug, ilo_bin, api_key)
        results.append(r)
        print(
            f"     outcome={r['outcome']}  "
            f"gen_tokens={r['generation_tokens']}  "
            f"attempts={r['attempts']}"
        )

    if args.baseline:
        record_baseline(results)
        return 0

    # Comparison mode
    if not BASELINE_FILE.exists():
        print(
            f"\nNo baseline found at {BASELINE_FILE}.\n"
            "Run with --baseline to record one first.",
            file=sys.stderr,
        )
        return 2

    baseline_data = json.loads(BASELINE_FILE.read_text())

    print("\nRegression comparison:")
    passed, failures = compare_to_baseline(results, baseline_data)

    print("\nSkill-module token sizes (current):")
    current_mod_tokens = module_token_report()
    baseline_mod_tokens = baseline_data.get("module_tokens", {})
    for name, tokens in current_mod_tokens.items():
        baseline_t = baseline_mod_tokens.get(name, tokens)
        delta = tokens - baseline_t
        flag = f"  (+{delta})" if delta > 0 else (f"  ({delta})" if delta < 0 else "")
        print(f"  {name:<26} {tokens:5d}{flag}")

    if failures:
        print("\nREGRESSIONS DETECTED:")
        for msg in failures:
            print(msg)
        return 1

    print("\nAll personas passed regression gate.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
