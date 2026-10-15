#!/usr/bin/env python3
"""
Closed-loop benchmark: ilo vs alternative language CLI (Phase 5, ILO-364).

Drives a full LLM → compile → repair → retry loop for each task on each
language (ilo and optionally a second CLI such as Zero), across both Haiku
and Sonnet model families.  Measures intent→green: spec, generation,
context, error feedback, and retries.

Per task, per language, per model this script logs:
  - thinking_tokens          provider thinking tokens, or null if no billed
                             attempt reported a split. A sum is a lower
                             bound when thinking_unknown_attempts > 0.
  - thinking_unknown_attempts
                             billed attempts whose response omitted the split
  - code_tokens              generation_tokens - thinking_tokens only when
                             every billed attempt reported a split; otherwise
                             null. Unknown thinking is not folded into code.
  - generated_chars          sum of len(emitted program) across billed
                             attempts (thinking blocks and markdown fences
                             are not program text)
  - final_code_chars         length of the last billed program
  - code_chars_by_turn       per billed attempt
  - generation_tokens        provider output_tokens sum. May include
                             thinking. Kept for cost continuity. Not a
                             density claim, and not the headline column.
  - input_tokens             uncached input + cache creation + cache read
  - input_cache_hit_tokens   cache_read_input_tokens sum (0 if absent)
  - input_cache_miss_tokens  uncached input + cache creation
  - repair_tokens_by_turn    provider output tokens of billed attempts
                             after the first
  - attempt_trace            per-turn code, stderr, thinking, served model
  - served_models            response model ids, first-seen order
  - attempts_to_success      attempts until working (null if never)
  - success_rate             1.0 / 0.0 per run
  - wall_time_s              wall-clock seconds
  - final_outcome            "working" | "partial" | "failed"

Thinking comes from usage.output_tokens_details.thinking_tokens when the
Messages API (or a CLI payload with the same field) sends an integer,
including 0. A missing or non-integer split is unknown. It is not guessed
from thinking-block length.

Output
  bench/closed-loop-<date>.json   structured JSON dataset
  bench/closed-loop-<date>.md     markdown writeup

Usage
  # ilo only, both models
  python3 scripts/closed-loop-bench.py

  # specify an ilo binary (default: ilo from PATH)
  python3 scripts/closed-loop-bench.py --ilo ./target/release/ilo

  # also benchmark a second language CLI
  python3 scripts/closed-loop-bench.py --lang2-name zero --lang2-bin zero --lang2-ext .zero

  # list tasks, no LLM calls
  python3 scripts/closed-loop-bench.py --dry-run

  # schema-shaped JSON with no API key (synthetic, not a measurement)
  python3 scripts/closed-loop-bench.py --emit-fixture bench/fixtures/closed-loop-harness-shape.json

  # after the metric schema (PR #797) is on the tree:
  python3 scripts/validate-closed-loop-results.py bench/fixtures/closed-loop-harness-shape.json

  # single task
  python3 scripts/closed-loop-bench.py --task simple-function

  # single model
  python3 scripts/closed-loop-bench.py --model haiku

Environment
  ANTHROPIC_API_KEY   required for a live run. Not required for --dry-run
                      or --emit-fixture.

Retry cap
  Default N=5.  Override with --retry-cap N.

Context arms (--context) are a separate change (PR #798). This harness
does not label a context arm.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
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
TRACE_IO_CHARS = 2000
REPAIR_CODE_CHARS = 8000

MODELS = {
    "haiku": "claude-haiku-4-5",
    "sonnet": "claude-sonnet-4-5",
}

# Outcome ranks for partial ordering
OUTCOME_RANK = {"working": 2, "partial": 1, "failed": 0}

# How to read a cell. Written into JSON and markdown so generation_tokens
# cannot be skimmed as a density result.
METRIC_NOTE = (
    "generation_tokens is the provider output sum and may include thinking. "
    "It is not a density claim. Honest columns are thinking_tokens and "
    "generated_chars (both emitted; thinking_tokens is null when no billed "
    "attempt reported a split). code_tokens is null when any billed attempt "
    "left thinking unknown."
)

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
    previous_code: str = "",
) -> str:
    """Repair turn. Includes the program that failed (repair memory)."""
    code = previous_code
    clipped = ""
    if len(code) > REPAIR_CODE_CHARS:
        code = code[:REPAIR_CODE_CHARS]
        clipped = "\n…[previous program truncated]\n"
    program = ""
    if code:
        program = f"Previous program:\n{code}{clipped}\n\n"
    return (
        f"The previous {lang} program for this task failed.\n"
        f"Task: {task['description']}\n"
        f"{program}"
        f"Error / actual output:\n{error}\n\n"
        f"Rewrite the program to fix the error. Output ONLY the {lang} code.\n"
        f"---LANGUAGE DOCUMENTATION---\n{context}\n---END---\n"
    )


# ---------------------------------------------------------------------------
# Provider payload → honest counts
# ---------------------------------------------------------------------------

def _nonneg_int(value: object) -> int | None:
    """Integer >= 0, or None. Bool is not an integer here."""
    if isinstance(value, bool) or not isinstance(value, int):
        return None
    if value < 0:
        return None
    return value


def strip_fences(text: str) -> str:
    """Strip a surrounding markdown code fence from model output.

    Same rule as scripts/persona-smoke.py. Fences are not program text,
    so they are removed before the run and before generated_chars.
    """
    stripped = text.strip()
    if stripped.startswith("```"):
        lines = stripped.split("\n")
        start = 1
        end = len(lines) - 1 if lines[-1].strip() == "```" else len(lines)
        return "\n".join(lines[start:end])
    return text


def message_text(body: dict[str, Any]) -> str:
    """Programme text only. Thinking blocks are not emitted code."""
    content = body.get("content")
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts: list[str] = []
        for block in content:
            if not isinstance(block, dict):
                continue
            kind = block.get("type")
            if kind in ("thinking", "redacted_thinking"):
                continue
            if kind == "text" or (kind is None and "text" in block):
                text = block.get("text")
                if isinstance(text, str):
                    parts.append(text)
        return "\n".join(parts)
    return ""


def extract_thinking_tokens(usage: dict[str, Any]) -> int | None:
    """Thinking tokens, or None when this payload has no integer split.

    A present 0 is a measured zero. A missing field, JSON null, or a
    non-integer is unknown — never coerced and never treated as zero.
    """
    details = usage.get("output_tokens_details")
    if isinstance(details, dict) and "thinking_tokens" in details:
        return _nonneg_int(details.get("thinking_tokens"))
    if "thinking_tokens" in usage:
        return _nonneg_int(usage.get("thinking_tokens"))
    return None


def parse_provider_turn(body: dict[str, Any]) -> dict[str, Any]:
    """Normalise one Messages API body (CLI payloads use the same split).

    input_tokens is the provider's uncached input count. Cache read and
    cache creation are separate so a missing cache field stays 0 (this
    request did not report cache) while a missing thinking split stays
    unknown.
    """
    usage = body.get("usage")
    if not isinstance(usage, dict):
        usage = {}
    model = body.get("model")
    if not isinstance(model, str) or not model.strip():
        model = None
    finish = body.get("stop_reason")
    if finish is not None and not isinstance(finish, str):
        finish = str(finish)
    uncached = _nonneg_int(usage.get("input_tokens"))
    return {
        "code": strip_fences(message_text(body)),
        "generation_tokens": _nonneg_int(usage.get("output_tokens")) or 0,
        "input_tokens": 0 if uncached is None else uncached,
        "thinking_tokens": extract_thinking_tokens(usage),
        "served_model": model,
        "cache_hit_tokens": _nonneg_int(usage.get("cache_read_input_tokens")) or 0,
        "cache_creation_tokens": (
            _nonneg_int(usage.get("cache_creation_input_tokens")) or 0
        ),
        "finish_reason": finish,
    }


@dataclass
class AttemptObs:
    """One loop turn. billed=False is a transport failure, not a generation."""

    code: str
    generation_tokens: int
    input_tokens: int
    thinking_tokens: int | None
    served_model: str | None
    cache_hit_tokens: int = 0
    cache_creation_tokens: int = 0
    stderr: str = ""
    stdout: str = ""
    rc: int | None = None
    outcome: str = "failed"
    finish_reason: str | None = None
    billed: bool = True
    api_error: str | None = None


def _clip(text: str, limit: int = TRACE_IO_CHARS) -> str:
    if len(text) <= limit:
        return text
    return text[:limit] + "\n…[truncated]"


def format_thinking(thinking: int | None, unknown: int) -> str:
    """Short cell for tables. Null is not printed as zero."""
    if thinking is None:
        return "unk"
    if unknown > 0:
        return f"≥{thinking}"
    return str(thinking)


def format_thinking_detail(thinking: int | None, unknown: int) -> str:
    if thinking is None:
        return (
            f"unknown ({unknown} billed attempt(s) omitted the split)"
        )
    if unknown > 0:
        return (
            f"{thinking} (lower bound; {unknown} billed attempt(s) "
            f"omitted the split)"
        )
    return str(thinking)


def format_code_tokens(code_tokens: int | None, unknown: int) -> str:
    if code_tokens is None:
        if unknown > 0:
            return "null (thinking unknown on at least one billed attempt)"
        return "null (thinking split not reported)"
    return str(code_tokens)


def assemble_cell(
    *,
    task_id: str,
    lang: str,
    model_key: str,
    model_id: str,
    attempts: list[AttemptObs],
    wall_time_s: float,
    final_outcome: str,
) -> dict[str, Any]:
    """Build one result cell. Transport failures are trace-only.

    code_tokens is generation_tokens - thinking_tokens only when every
    billed attempt reported an integer thinking split. Otherwise it is
    null, including when the known sum is a lower bound.
    """
    billed = [a for a in attempts if a.billed]
    generation_tokens = sum(a.generation_tokens for a in billed)
    known = [a.thinking_tokens for a in billed if a.thinking_tokens is not None]
    unknown = sum(1 for a in billed if a.thinking_tokens is None)
    thinking_tokens: int | None = sum(known) if known else None
    if unknown == 0 and thinking_tokens is not None:
        code_tokens: int | None = generation_tokens - thinking_tokens
    else:
        code_tokens = None

    code_chars_by_turn = [len(a.code) for a in billed]
    generated_chars = sum(code_chars_by_turn)
    final_code_chars = code_chars_by_turn[-1] if code_chars_by_turn else 0
    repair_tokens_by_turn = [a.generation_tokens for a in billed[1:]]

    cache_hit = sum(a.cache_hit_tokens for a in billed)
    cache_creation = sum(a.cache_creation_tokens for a in billed)
    uncached = sum(a.input_tokens for a in billed)
    cache_miss = uncached + cache_creation
    input_tokens = cache_miss + cache_hit

    served: list[str] = []
    for a in billed:
        if a.served_model and a.served_model not in served:
            served.append(a.served_model)

    trace: list[dict[str, Any]] = []
    for i, a in enumerate(attempts, start=1):
        trace.append({
            "attempt": i,
            "billed": a.billed,
            "code": a.code,
            "code_chars": len(a.code) if a.billed else 0,
            "stderr": _clip(a.stderr),
            "stdout": _clip(a.stdout),
            "rc": a.rc,
            "outcome": a.outcome,
            "thinking_tokens": a.thinking_tokens,
            "generation_tokens": a.generation_tokens,
            "input_tokens": a.input_tokens,
            "cache_hit_tokens": a.cache_hit_tokens,
            "cache_creation_tokens": a.cache_creation_tokens,
            "served_model": a.served_model,
            "finish_reason": a.finish_reason,
            "api_error": a.api_error,
        })

    if final_outcome == "working":
        attempts_to_success: int | None = len(attempts)
    else:
        attempts_to_success = None

    return {
        "task": task_id,
        "language": lang,
        "model": model_key,
        "model_id": model_id,
        "served_models": served,
        "generation_tokens": generation_tokens,
        "thinking_tokens": thinking_tokens,
        "thinking_unknown_attempts": unknown,
        "code_tokens": code_tokens,
        "generated_chars": generated_chars,
        "final_code_chars": final_code_chars,
        "code_chars_by_turn": code_chars_by_turn,
        "input_tokens": input_tokens,
        "input_cache_hit_tokens": cache_hit,
        "input_cache_miss_tokens": cache_miss,
        "repair_tokens_by_turn": repair_tokens_by_turn,
        "attempts_to_success": attempts_to_success,
        "attempts_total": len(attempts),
        "success_rate": 1.0 if final_outcome == "working" else 0.0,
        "wall_time_s": round(float(wall_time_s), 2),
        "final_outcome": final_outcome,
        "attempt_trace": trace,
    }


def fixture_payload() -> dict[str, Any]:
    """Synthetic cells in the live result shape. Not a measurement.

    Cell 0: every attempt reported thinking, so code_tokens is the
    difference. Cell 1: one attempt omitted the split, so thinking_tokens
    is only the known sum and code_tokens is null. Both carry
    generated_chars.
    """
    split = AttemptObs(
        code="-- run: main\n",
        generation_tokens=120,
        input_tokens=800,
        thinking_tokens=90,
        served_model="claude-haiku-4-5",
        cache_hit_tokens=0,
        cache_creation_tokens=0,
        stdout="ok\n",
        rc=0,
        outcome="working",
        finish_reason="end_turn",
        billed=True,
    )
    unknown = AttemptObs(
        code="x\n",
        generation_tokens=40,
        input_tokens=90,
        thinking_tokens=None,
        served_model="claude-haiku-4-5",
        stderr="ILO-P011\n",
        rc=1,
        outcome="failed",
        finish_reason="end_turn",
        billed=True,
    )
    known = AttemptObs(
        code="y\n",
        generation_tokens=50,
        input_tokens=100,
        thinking_tokens=10,
        served_model="claude-haiku-4-5-20251001",
        cache_hit_tokens=20,
        cache_creation_tokens=5,
        stdout="ok\n",
        rc=0,
        outcome="working",
        finish_reason="end_turn",
        billed=True,
    )
    return {
        "generated": "fixture",
        "harness": "closed-loop-bench.py --emit-fixture",
        "ticket": "ILO-364",
        "note": (
            "Synthetic cells for the honest metric shape. Not a measurement. "
            + METRIC_NOTE
            + " Validate with scripts/validate-closed-loop-results.py "
            "(bench/metric-schema.json, PR #797)."
        ),
        "results": [
            assemble_cell(
                task_id="simple-function",
                lang="ilo",
                model_key="haiku",
                model_id="claude-haiku-4-5",
                attempts=[split],
                wall_time_s=1.25,
                final_outcome="working",
            ),
            assemble_cell(
                task_id="simple-function",
                lang="ilo",
                model_key="haiku",
                model_id="claude-haiku-4-5",
                attempts=[unknown, known],
                wall_time_s=2.5,
                final_outcome="working",
            ),
        ],
    }


def write_fixture(path: str) -> Path | None:
    """Write fixture_payload(). path '-' writes to stdout and returns None."""
    text = json.dumps(fixture_payload(), indent=2) + "\n"
    if path == "-":
        sys.stdout.write(text)
        return None
    dest = Path(path)
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(text)
    return dest


# ---------------------------------------------------------------------------
# API call
# ---------------------------------------------------------------------------

def call_llm(
    system: str,
    user: str,
    model_id: str,
    api_key: str,
) -> dict[str, Any]:
    """One Messages API turn, parsed by parse_provider_turn.

    Raises on transport or HTTP error. Does not invent a thinking split
    when the body omits one. The requested model_id is not copied into
    served_model; that field is only the response's model id.
    """
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
    if not isinstance(body, dict):
        raise ValueError("Messages API response was not a JSON object")
    return parse_provider_turn(body)


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

    observations: list[AttemptObs] = []
    outcome = "failed"
    wall_start = time.monotonic()

    for attempt in range(1, retry_cap + 1):
        try:
            parsed = call_llm(system, user, model_id, api_key)
        except Exception as exc:  # noqa: BLE001
            print(f"      [attempt {attempt}] API error: {exc}", file=sys.stderr)
            observations.append(AttemptObs(
                code="",
                generation_tokens=0,
                input_tokens=0,
                thinking_tokens=None,
                served_model=None,
                outcome="failed",
                billed=False,
                api_error=str(exc),
            ))
            time.sleep(2)
            continue

        code = parsed["code"]
        stdout, stderr, rc = run_fn(code)
        outcome = classify_outcome(task["expected_output"], stdout, stderr, rc)
        think = parsed["thinking_tokens"]
        observations.append(AttemptObs(
            code=code,
            generation_tokens=parsed["generation_tokens"],
            input_tokens=parsed["input_tokens"],
            thinking_tokens=think,
            served_model=parsed["served_model"],
            cache_hit_tokens=parsed["cache_hit_tokens"],
            cache_creation_tokens=parsed["cache_creation_tokens"],
            stderr=stderr,
            stdout=stdout,
            rc=rc,
            outcome=outcome,
            finish_reason=parsed["finish_reason"],
            billed=True,
        ))

        print(
            f"      attempt={attempt} outcome={outcome} "
            f"thinking={format_thinking(think, 0 if think is not None else 1)} "
            f"code_chars={len(code)} "
            f"provider_output_tokens={parsed['generation_tokens']} rc={rc}",
            file=sys.stderr,
        )

        if outcome == "working":
            break

        error_detail = (stderr or stdout or "(no output)").strip()[:1000]
        user = make_repair_prompt(task, context, error_detail, lang, code)

    wall_time = time.monotonic() - wall_start
    return assemble_cell(
        task_id=task["id"],
        lang=lang,
        model_key=model_key,
        model_id=model_id_used,
        attempts=observations,
        wall_time_s=wall_time,
        final_outcome=outcome,
    )


# ---------------------------------------------------------------------------
# Output helpers
# ---------------------------------------------------------------------------

def write_json(results: list[dict[str, Any]], date_str: str) -> Path:
    out = BENCH_DIR / f"closed-loop-{date_str}.json"
    payload = {
        "generated": datetime.now(timezone.utc).isoformat(),
        "harness": "closed-loop-bench.py",
        "ticket": "ILO-364",
        "note": METRIC_NOTE,
        "results": results,
    }
    out.write_text(json.dumps(payload, indent=2) + "\n")
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

    comparators = [lang for lang in langs_seen if lang != "ilo"]
    if len(comparators) == 1:
        versus = comparators[0]
    elif comparators:
        versus = ", ".join(comparators)
    else:
        versus = "comparator"
    lines: list[str] = [
        f"# Closed-loop benchmark: ilo vs {versus}",
        "",
        f"Generated: {datetime.now(timezone.utc).strftime('%Y-%m-%d %H:%M UTC')}  ",
        "Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  ",
        f"Retry cap: {DEFAULT_RETRY_CAP}",
        "",
        "## How to read this",
        "",
        METRIC_NOTE,
        "",
        "The table leads with thinking tokens and emitted-code characters.",
        "`unk` means no billed attempt reported a thinking split.",
        "`≥n` means n is a lower bound: some billed attempts omitted the split,",
        "and those attempts are not counted as zero thinking.",
        "`chars` is the length of the program text after fence stripping,",
        "summed across billed attempts. Thinking-block text is not included.",
        "",
        "## Summary",
        "",
    ]

    combos = [(lang, model) for lang in langs_seen for model in models_seen]
    lines.append("| task | " + " | ".join(
        f"{lang}/{model}: think | chars | inp | att | time | outcome"
        for lang, model in combos
    ) + " |")
    lines.append("|---" * (1 + len(combos) * 6) + "|")

    for task in tasks_seen:
        row = f"| {task} |"
        for lang, model in combos:
            r = idx.get((task, lang, model))
            if r:
                att = str(r["attempts_to_success"]) if r["attempts_to_success"] else "-"
                think = format_thinking(
                    r.get("thinking_tokens"),
                    int(r.get("thinking_unknown_attempts") or 0),
                )
                row += (
                    f" {think} |"
                    f" {r.get('generated_chars', '—')} |"
                    f" {r['input_tokens']} |"
                    f" {att} |"
                    f" {r['wall_time_s']}s |"
                    f" {r['final_outcome']} |"
                )
            else:
                row += " - | - | - | - | - | - |"
        lines.append(row)

    lines += [
        "",
        "## Per-task details",
        "",
    ]

    for task in tasks_seen:
        lines.append(f"### {task}")
        for lang in langs_seen:
            for model in models_seen:
                r = idx.get((task, lang, model))
                if not r:
                    continue
                unknown = int(r.get("thinking_unknown_attempts") or 0)
                lines += [
                    "",
                    f"**{lang} / {model}**  ",
                    f"- Thinking tokens: {format_thinking_detail(r.get('thinking_tokens'), unknown)}  ",
                    f"- Emitted code chars: {r.get('generated_chars')}  ",
                    f"- Final code chars: {r.get('final_code_chars')}  ",
                    f"- Code chars by turn: {r.get('code_chars_by_turn')}  ",
                    f"- Code tokens: {format_code_tokens(r.get('code_tokens'), unknown)}  ",
                    (
                        f"- Provider output tokens (generation_tokens, "
                        f"not a density claim): {r['generation_tokens']}  "
                    ),
                    f"- Input tokens (context): {r['input_tokens']}  ",
                    f"- Input cache hit tokens: {r.get('input_cache_hit_tokens', 0)}  ",
                    f"- Input cache miss tokens: {r.get('input_cache_miss_tokens', 0)}  ",
                    f"- Served models: {r.get('served_models', [])}  ",
                    f"- Attempts total: {r['attempts_total']}  ",
                    f"- Attempts to success: {r['attempts_to_success']}  ",
                    f"- Repair tokens by turn: {r['repair_tokens_by_turn']}  ",
                    f"- Wall time: {r['wall_time_s']}s  ",
                    f"- Outcome: **{r['final_outcome']}**  ",
                ]
                trace = r.get("attempt_trace") or []
                if trace:
                    lines.append("- Attempts:  ")
                    for turn in trace:
                        th = turn.get("thinking_tokens")
                        th_s = "unknown" if th is None else str(th)
                        err = ""
                        if turn.get("api_error"):
                            err = f" api_error={turn['api_error']}"
                        lines.append(
                            f"  - attempt {turn.get('attempt')}: "
                            f"outcome={turn.get('outcome')} "
                            f"thinking={th_s} "
                            f"chars={turn.get('code_chars')} "
                            f"served={turn.get('served_model') or '-'} "
                            f"billed={turn.get('billed')}{err}  "
                        )
        lines.append("")

    lines += [
        "## Notes",
        "",
        "- Headline columns are thinking and emitted-code characters.",
        "  generation_tokens stays on the JSON cell and in the detail list",
        "  so a cost sum is still possible. Do not quote it as density.",
        "- Skill documentation is loaded once per process (steady-state caching).",
        "- Repair turns include the previous program.",
        "- Re-run at any time; output files are date-stamped.",
        "- Context arm labels are not written by this revision",
        "  (see the context-modes change).",
        "",
    ]

    out.write_text("\n".join(lines) + "\n")
    return out


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------

HOWTO_FIXTURE = """\
Schema-valid JSON without an API key (synthetic, not a measurement):
  python3 scripts/closed-loop-bench.py --emit-fixture bench/fixtures/closed-loop-harness-shape.json
With the metric schema and validator (PR #797):
  python3 scripts/validate-closed-loop-results.py bench/fixtures/closed-loop-harness-shape.json
A cell that has only generation_tokens fails that validator.
"""


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
    parser.add_argument(
        "--emit-fixture",
        metavar="PATH",
        help=(
            "Write a synthetic schema-shaped JSON file and do not call a model. "
            "PATH '-' writes to stdout. No API key. Not a measurement."
        ),
    )
    parser.add_argument("--output-dir", default=None,
                        help="Override output directory (default: bench/).")
    args = parser.parse_args()

    global BENCH_DIR
    if args.output_dir:
        BENCH_DIR = Path(args.output_dir)

    if args.emit_fixture:
        dest = write_fixture(args.emit_fixture)
        if dest is not None:
            print(f"Wrote schema-shaped fixture (not a measurement): {dest}")

    # Fixture-only does not read the task list. Dry-run and live runs do.
    if args.emit_fixture and not args.dry_run:
        return 0

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
        print()
        print(HOWTO_FIXTURE, end="")
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
                    f"thinking={format_thinking(r['thinking_tokens'], r['thinking_unknown_attempts'])} "
                    f"chars={r['generated_chars']} "
                    f"provider_output_tokens={r['generation_tokens']} "
                    f"wall={r['wall_time_s']}s"
                )

    date_str = datetime.now(timezone.utc).strftime("%Y-%m-%d")
    json_path = write_json(results, date_str)
    md_path = write_markdown(results, date_str)

    print("\nResults written:")
    print(f"  JSON: {json_path}")
    print(f"  MD:   {md_path}")

    # Summary table to stdout. Thinking and chars lead; the provider
    # output sum is not a column.
    print("\nSummary (thinking and emitted chars; provider output is in the JSON):")
    print(
        f"{'task':<22} {'lang':<6} {'model':<6} "
        f"{'think':>8} {'chars':>7} {'attempts':>8} {'outcome':<8} {'time':>6}"
    )
    print("-" * 80)
    for r in results:
        att = str(r["attempts_to_success"]) if r["attempts_to_success"] else "-"
        think = format_thinking(r["thinking_tokens"], r["thinking_unknown_attempts"])
        print(
            f"{r['task']:<22} {r['language']:<6} {r['model']:<6} "
            f"{think:>8} {r['generated_chars']:>7} {att:>8} {r['final_outcome']:<8} "
            f"{r['wall_time_s']:>5.1f}s"
        )

    success_count = sum(1 for r in results if r["final_outcome"] == "working")
    print(f"\n{success_count}/{total_runs} runs succeeded.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
