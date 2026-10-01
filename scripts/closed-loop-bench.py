#!/usr/bin/env python3
"""
Closed-loop benchmark: ilo vs alternative language CLI (Phase 5, ILO-364).

Drives a full LLM → compile → repair → retry loop for each task on each
language (ilo and optionally a second CLI such as Zero). Measures
intent→green: spec, generation, context, error feedback, and retries.

A live run prefers DeepSeek (OpenAI-compatible Chat Completions) when
DEEPSEEK_API_KEY is set. Anthropic Haiku and Sonnet remain available with
--provider anthropic.

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
  bench/closed-loop-<date>.json          ilo-only JSON
  bench/closed-loop-<date>.md            ilo-only markdown
  bench/closed-loop-<date>-<leg>.json    comparator leg (python, bash, …)
                                         so interleaved arms do not clobber

Usage
  # ilo only, both models
  python3 scripts/closed-loop-bench.py

  # specify an ilo binary (default: ilo from PATH)
  python3 scripts/closed-loop-bench.py --ilo ./target/release/ilo

  # also benchmark a second language CLI
  python3 scripts/closed-loop-bench.py --lang2-name zero --lang2-bin zero --lang2-ext .zero

  # Python arm (name python, binary python3, extension .py)
  python3 scripts/closed-loop-bench.py --python

  # bash arm. The runner is `bash <file>.sh` (the same [bin, file] shape).
  python3 scripts/closed-loop-bench.py --bash
  python3 scripts/closed-loop-bench.py --lang2-name bash --lang2-bin bash --lang2-ext .sh

  # fair comparator docs (without this, lang2 is a memorised-prior stub)
  python3 scripts/closed-loop-bench.py --python --lang2-docs path/to/python.md

  # list tasks, no LLM calls
  python3 scripts/closed-loop-bench.py --dry-run
  python3 scripts/closed-loop-bench.py --dry-run --python

  # schema-shaped JSON with no API key (synthetic, not a measurement)
  python3 scripts/closed-loop-bench.py --emit-fixture bench/fixtures/closed-loop-harness-shape.json

  # after the metric schema (PR #797) is on the tree:
  python3 scripts/validate-closed-loop-results.py bench/fixtures/closed-loop-harness-shape.json

  # single task
  python3 scripts/closed-loop-bench.py --task simple-function

  # single Anthropic model
  python3 scripts/closed-loop-bench.py --provider anthropic --model haiku

DeepSeek (preferred live provider when DEEPSEEK_API_KEY is set)
  # no key, no network: prints the Chat Completions URL and the wire model
  python3 scripts/closed-loop-bench.py --dry-run --provider deepseek --model deepseek-chat

  # one task. The key stays in the environment; do not commit it.
  python3 scripts/closed-loop-bench.py --provider deepseek --model deepseek-chat --task simple-function
  python3 scripts/closed-loop-bench.py --provider deepseek --model deepseek-reasoner --task simple-function

  # current ids, sent as themselves (thinking field omitted)
  python3 scripts/closed-loop-bench.py --provider deepseek --model deepseek-flash --task simple-function
  python3 scripts/closed-loop-bench.py --provider deepseek --model deepseek-v4-pro --task simple-function

  # with DEEPSEEK_API_KEY set and no --provider / --model, the live path is
  # deepseek-chat (one model). --model both --provider deepseek runs
  # deepseek-chat and deepseek-reasoner.
  python3 scripts/closed-loop-bench.py --task simple-function

  deepseek-chat and deepseek-reasoner are CLI names. The request body sends
  model deepseek-flash with thinking.type disabled (chat) or enabled
  (reasoner). DeepSeek discontinued those two model ids on 2026-07-24
  (changelog 2026-04-24). The cell records model (the CLI key) and model_id
  (the id on the wire). Another deepseek-* id is sent unchanged.

Context arms (ilo skill modules). --dry-run needs no API key.
  Default is curated. full is the explicit balloon control.
  Historical runs before this flag always loaded
  ilo-language + ilo-builtins-core + ilo-builtins-text +
  ilo-builtins-math + ilo-builtins-io and wrote no arm label.
  That set is not a named mode. full also loads ilo-builtins-sig.

  python3 scripts/closed-loop-bench.py --dry-run --context core
  python3 scripts/closed-loop-bench.py --dry-run --context curated
  python3 scripts/closed-loop-bench.py --dry-run --context full
  python3 scripts/closed-loop-bench.py --dry-run --context task-modules
  python3 scripts/closed-loop-bench.py --dry-run --modules-from-task

  core          ilo-language, ilo-builtins-core
  curated       core + ilo-builtins-sig
  full          curated + ilo-builtins-io, ilo-builtins-text, ilo-builtins-math
  task-modules  exactly task["modules"] in bench/closed-loop/tasks.json
                (errors if that list is missing)

  ilo-builtins-sig is read from bench/closed-loop/context/ (not an
  `ilo skill list` entry). Other modules prefer `ilo skill get`, then
  skills/ilo/<name>.md.

  Every result cell records "context" (the arm) and "context_modules".
  This flag cuts which documentation is loaded (manifesto principle 3,
  self-contained context). It does not by itself report a density result.

Repair shape hint (exp-03). Default off, so a baseline arm is today's
repair text. --repair-shape-hint appends a fixed header gloss inside
make_repair_prompt when the repair stderr contains ILO-P003, or the
paren-header pair (expected `>`, got `(`). The gloss names
`name params>ret;body`, contrasts `main()>…` with `main>_;…`, and gives
one correct one-liner (`tri n:n>n;+n 1` then `main>_;prnt (tri 10)`).
Call sites may still use `(…)`. The initial prompt is left as-is.
This is a retry cut (manifesto principle 6: structured compiler→agent
surface). It is not a density claim. --dry-run prints the flag and
does not need an API key.

  # baseline (flag off)
  python3 scripts/closed-loop-bench.py --provider deepseek --model deepseek-chat \
    --context curated --retry-cap 2 --output-dir bench/exp03-f0-baseline/

  # augmented repair prompt
  python3 scripts/closed-loop-bench.py --provider deepseek --model deepseek-chat \
    --context curated --retry-cap 2 --repair-shape-hint \
    --output-dir bench/exp03-f1-shape-hint/

  python3 scripts/closed-loop-bench.py --dry-run --repair-shape-hint

Environment
  DEEPSEEK_API_KEY    live DeepSeek runs. Preferred when set and neither
                      --provider nor an Anthropic model is requested.
                      Not required for --dry-run or --emit-fixture.
  DEEPSEEK_BASE_URL   Chat Completions origin. Default
                      https://api.deepseek.com. The harness posts
                      {base}/chat/completions. DEEPSEEK_API_BASE is the
                      same setting. A base that already ends in
                      /chat/completions is used as the full URL.
                      The Anthropic-compatible origin
                      (https://api.deepseek.com/anthropic) is a different
                      API; this harness does not call it.
  ANTHROPIC_API_KEY   live Anthropic runs (--provider anthropic, or
                      --model haiku / sonnet / both when DeepSeek was not
                      selected). Not required for --dry-run or
                      --emit-fixture.

DeepSeek usage (best effort, not guessed)
  generation_tokens   usage.completion_tokens (else usage.output_tokens)
  thinking_tokens     usage.completion_tokens_details.reasoning_tokens
                      when that value is a non-negative integer, including
                      0. Missing, null, or non-integer stays null. A 0 is
                      a measured zero. reasoning_content is not a token
                      count and is not programme text.
  generated_chars     length of choices[0].message.content after fence
                      stripping, summed across billed attempts.
  max_tokens          8192 on DeepSeek. Thinking and the program share
                      that cap; finish_reason "length" means it was hit.
                      Anthropic stays at 1024.
  input               prompt_cache_miss_tokens is the uncached count and
                      prompt_cache_hit_tokens is the cache hit when both
                      are integers. prompt_tokens is their sum and is not
                      added again.

Retry cap
  Default N=5.  Override with --retry-cap N.

Every result cell records the context arm (`context`, `context_modules`),
the language arm (`lang_arm`, `docs_source`, `fair_docs`, `task_class`),
and the honest columns above. generation_tokens is not a density claim.
An ops row, or a bash win on wall time, is not an ilo manifesto loss.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, NamedTuple

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

REPO_ROOT = Path(__file__).resolve().parent.parent
BENCH_DIR = REPO_ROOT / "bench"
TASKS_FILE = BENCH_DIR / "closed-loop" / "tasks.json"
TASK_CLASS_FILE = BENCH_DIR / "closed-loop" / "task-class.json"
SKILLS_DIR = REPO_ROOT / "skills" / "ilo"
# Signature sheet for the curated/full arms. Not registered with
# `ilo skill list`; the harness loads it by module name.
CONTEXT_DIR = BENCH_DIR / "closed-loop" / "context"

# Fixed arms. task-modules is resolved per task from tasks.json.
CONTEXT_MODULES: dict[str, list[str]] = {
    "core": ["ilo-language", "ilo-builtins-core"],
    "curated": ["ilo-language", "ilo-builtins-core", "ilo-builtins-sig"],
    "full": [
        "ilo-language",
        "ilo-builtins-core",
        "ilo-builtins-sig",
        "ilo-builtins-io",
        "ilo-builtins-text",
        "ilo-builtins-math",
    ],
}
CONTEXT_MODES = ("core", "curated", "full", "task-modules")
DEFAULT_CONTEXT = "curated"

DEFAULT_RETRY_CAP = 5
ILO_TIMEOUT = 20  # seconds per ilo run
LANG2_TIMEOUT = 20  # seconds per secondary language run
TRACE_IO_CHARS = 2000
REPAIR_CODE_CHARS = 8000

MODELS = {
    "haiku": "claude-haiku-4-5",
    "sonnet": "claude-sonnet-4-5",
}
ANTHROPIC_MODEL_KEYS = ("haiku", "sonnet")
ANTHROPIC_MESSAGES_URL = "https://api.anthropic.com/v1/messages"
ANTHROPIC_MAX_TOKENS = 1024

# OpenAI-compatible Chat Completions. Not the Anthropic Messages shim.
DEFAULT_DEEPSEEK_BASE = "https://api.deepseek.com"
DEEPSEEK_MAX_TOKENS = 8192
DEEPSEEK_TIMEOUT_S = 180
# One model when DeepSeek is selected and --model is omitted. "both" on
# this provider is an explicit pair (chat + reasoner), not the default.
DEFAULT_DEEPSEEK_MODEL = "deepseek-chat"

# CLI key -> (wire model id, thinking request).
# thinking is "enabled", "disabled", or None (omit the field).
# deepseek-chat / deepseek-reasoner were discontinued as API model ids on
# 2026-07-24. Until then they meant non-thinking and thinking modes of
# flash. The live flash id is deepseek-flash (V4.1). The CLI names stay;
# the body uses the live id and an explicit thinking mode.
DEEPSEEK_MODEL_PLAN: dict[str, tuple[str, str | None]] = {
    "deepseek-chat": ("deepseek-flash", "disabled"),
    "deepseek-reasoner": ("deepseek-flash", "enabled"),
    "deepseek-flash": ("deepseek-flash", None),
    "deepseek-v4-flash": ("deepseek-v4-flash", None),
    "deepseek-v4-pro": ("deepseek-v4-pro", None),
}
PROVIDERS = ("anthropic", "deepseek")

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

# Ops and wall-clock wins for bash are expected. They are not an ilo loss.
NICHE_STANCE = (
    "language-of-record bakeoff; an ops task or a bash win on wall time "
    "is not an ilo manifesto loss"
)
VALID_TASK_CLASSES = frozenset({"artefact", "ops", "sanity"})
EXT_BY_LANG = {"python": ".py", "bash": ".sh"}
# Names that must not appear in task text sent to every arm.
KNOWN_LANGS = frozenset({
    "ilo", "zero", "moonbit", "ailang", "nanolang", "bash", "python",
    "rust", "javascript", "typescript", "node",
})

class Lang2(NamedTuple):
    name: str
    bin: str
    ext: str


def resolve_lang2(
    python: bool,
    bash: bool,
    name: str | None,
    bin_path: str | None,
    ext: str | None,
) -> tuple[Lang2 | None, str | None]:
    """Resolve the optional comparator arm. Returns (arm, error).

    ``--python`` is name ``python``, binary ``python3``, extension ``.py``.
    ``--bash`` is name ``bash``, binary ``bash``, extension ``.sh``.
    ``--lang2-name bash --lang2-bin bash --lang2-ext .sh`` is the same bash arm.
    An omitted extension is ``.py`` / ``.sh`` for those names and ``.zero``
    otherwise (the historical default).
    """
    if python and bash:
        return None, "pass only one of --python and --bash"
    if python:
        if name and name != "python":
            return None, f"--python sets the comparator to python, not {name}"
        name = "python"
        bin_path = bin_path or "python3"
    elif bash:
        if name and name != "bash":
            return None, f"--bash sets the comparator to bash, not {name}"
        name = "bash"
        bin_path = bin_path or "bash"
    if not name and not bin_path:
        return None, None
    if name and not bin_path:
        return None, (
            f"--lang2-name {name} needs --lang2-bin "
            "(or use --python / --bash)"
        )
    if bin_path and not name:
        return None, "--lang2-bin needs --lang2-name (or use --python / --bash)"
    assert name is not None and bin_path is not None
    if not re.fullmatch(r"[A-Za-z0-9._-]+", name):
        return None, f"comparator name is not a safe filename leg: {name}"
    if ext is None:
        ext = EXT_BY_LANG.get(name, ".zero")
    if not ext.startswith("."):
        return None, f"--lang2-ext must start with '.': {ext}"
    return Lang2(name, bin_path, ext), None


def lang2_documentation(
    lang: str, docs_path: str | None,
) -> tuple[str, str, bool, str | None]:
    """Return (text, docs_source, fair_docs, error).

    No ``--lang2-docs`` means the memorised-prior stub. That arm is not a
    fair bakeoff against ilo skill text. A path that was passed and is
    missing or empty is an error: the run does not quietly substitute the
    stub after the operator asked for a file.
    """
    if not docs_path:
        text = f"(No formal language documentation available for {lang}.)"
        return text, "stub", False, None
    path = Path(docs_path)
    if not path.is_file():
        return "", "missing", False, f"--lang2-docs is not a file: {docs_path}"
    text = path.read_text()
    if not text.strip():
        return "", "empty", False, f"--lang2-docs is empty: {docs_path}"
    return text, "file", True, None


def check_language_neutral(
    tasks: list[dict[str, Any]],
    langs: list[str] | tuple[str, ...] = (),
) -> list[str]:
    """Task ids whose description names a language.

    The description is sent verbatim to every arm, so naming one language
    biases the others. ``langs`` adds whatever this invocation will run.
    """
    names = set(KNOWN_LANGS)
    names.update(str(lang).lower() for lang in langs if lang)
    bad: list[str] = []
    for task in tasks:
        words = set(re.findall(r"[a-z0-9]+", task["description"].lower()))
        if words & names:
            bad.append(task["id"])
    return bad


def load_task_classes(path: Path | None = None) -> dict[str, str]:
    """Hand tags: artefact | ops | sanity. Missing file yields {}."""
    path = path or TASK_CLASS_FILE
    if not path.is_file():
        return {}
    data = json.loads(path.read_text())
    classes = data.get("classes", {})
    if not isinstance(classes, dict):
        raise ValueError(f"{path}: 'classes' must be an object")
    bad = [f"{key}={value}" for key, value in classes.items()
           if value not in VALID_TASK_CLASSES]
    if bad:
        raise ValueError(
            "task_class must be artefact, ops, or sanity: " + ", ".join(bad)
        )
    return {str(key): str(value) for key, value in classes.items()}


def task_class_of(task: dict[str, Any], classes: dict[str, str]) -> str | None:
    """Inline ``task_class`` wins over the sidecar. Unknown values error."""
    inline = task.get("task_class")
    if inline is not None:
        if inline not in VALID_TASK_CLASSES:
            raise ValueError(
                f"{task.get('id')}: task_class {inline!r} "
                "is not artefact|ops|sanity"
            )
        return str(inline)
    found = classes.get(task["id"])
    return found


def probe_lang_bin(bin_path: str) -> tuple[bool, str]:
    """Confirm a comparator CLI can execute a file.

    ``python3 --version`` and ``bash --version`` both exit 0. A strict
    POSIX ``sh`` (dash) rejects ``--version`` and still runs ``sh file``.
    A non-zero version probe falls back to ``bin -c 'exit 0'``, which
    dash, bash, and python3 all accept, so the version-flag quirk does
    not drop the arm.
    """
    if os.path.sep not in bin_path:
        resolved = shutil.which(bin_path)
        if resolved is None:
            return False, f"not on PATH: {bin_path}"
        exe = resolved
    else:
        exe = bin_path
        if not os.path.isfile(exe):
            return False, f"not found: {bin_path}"
    try:
        ver = subprocess.run(
            [exe, "--version"],
            capture_output=True, text=True, timeout=5,
        )
    except FileNotFoundError:
        return False, f"not found: {bin_path}"
    except subprocess.TimeoutExpired:
        return False, f"timed out: {bin_path} --version"
    except OSError as exc:
        return False, f"cannot exec {bin_path}: {exc}"
    if ver.returncode == 0:
        line = (ver.stdout or ver.stderr or "").strip().splitlines()
        detail = line[0] if line else f"{exe} --version ok"
        return True, detail
    try:
        smoke = subprocess.run(
            [exe, "-c", "exit 0"],
            capture_output=True, text=True, timeout=5,
        )
    except (FileNotFoundError, subprocess.TimeoutExpired, OSError) as exc:
        return False, (
            f"{bin_path} --version exited {ver.returncode}; smoke failed: {exc}"
        )
    if smoke.returncode == 0:
        return True, (
            f"{exe} (version flag exited {ver.returncode}; -c ok)"
        )
    return False, (
        f"{bin_path} --version exited {ver.returncode} "
        f"and -c exited {smoke.returncode}"
    )


def result_stem(date_str: str, leg: str | None) -> str:
    """ilo-only keeps the historical name. A comparator leg is suffixed."""
    if leg:
        return f"closed-loop-{date_str}-{leg}"
    return f"closed-loop-{date_str}"


# ---------------------------------------------------------------------------
# ILO skill context (cached once per process to match steady-state economics)
# ---------------------------------------------------------------------------

_SKILL_CACHE: dict[str, str] = {}


class ContextError(Exception):
    """Context arm could not be built. The message is safe to print."""


def load_skill_text(module_name: str, ilo_bin: str) -> str:
    """Load one skill module, caching in memory (steady-state: single load).

    Prefers `ilo skill get`, then skills/ilo/<name>.md, then the harness
    context directory. Raises ContextError when none of those yield text.
    A missing module is not replaced with a stub: a stub would look like a
    small context arm.
    """
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
    for base in (SKILLS_DIR, CONTEXT_DIR):
        path = base / f"{module_name}.md"
        if path.is_file():
            text = path.read_text()
            _SKILL_CACHE[module_name] = text
            return text
    raise ContextError(
        f"context module {module_name!r} not found "
        f"(no `ilo skill get` text, and neither "
        f"{SKILLS_DIR / (module_name + '.md')} nor "
        f"{CONTEXT_DIR / (module_name + '.md')} exists)"
    )


def resolve_context_mode(context: str | None, modules_from_task: bool) -> str:
    """Pick the context arm. Default is curated.

    --modules-from-task is the task-modules arm. Combining it with a
    different --context value is an error.
    """
    if modules_from_task and context not in (None, "task-modules"):
        raise ContextError(
            "--modules-from-task selects context arm task-modules; "
            f"do not combine it with --context {context}"
        )
    if modules_from_task or context == "task-modules":
        return "task-modules"
    if context is None:
        return DEFAULT_CONTEXT
    if context not in CONTEXT_MODULES:
        raise ContextError(f"unknown context mode {context!r}")
    return context


def modules_for(context_mode: str, task: dict[str, Any] | None = None) -> list[str]:
    """Module names loaded for this arm.

    task-modules joins task["modules"] and errors when that list is missing
    or empty. Fixed arms ignore the task.
    """
    if context_mode == "task-modules":
        if task is None:
            raise ContextError("task-modules requires a task")
        mods = task.get("modules")
        task_id = task.get("id", "?")
        if (
            not isinstance(mods, list)
            or not mods
            or not all(isinstance(m, str) and m.strip() for m in mods)
        ):
            raise ContextError(
                f"--context task-modules: task {task_id!r} has no non-empty "
                f'"modules" list in {TASKS_FILE}'
            )
        return [m.strip() for m in mods]
    try:
        return list(CONTEXT_MODULES[context_mode])
    except KeyError as exc:
        raise ContextError(f"unknown context mode {context_mode!r}") from exc


def ilo_context(
    ilo_bin: str,
    context_mode: str = DEFAULT_CONTEXT,
    task: dict[str, Any] | None = None,
) -> str:
    """Return the ilo skill documentation for one context arm (cached)."""
    mods = modules_for(context_mode, task)
    return "\n\n".join(load_skill_text(m, ilo_bin) for m in mods)


def prepare_ilo_contexts(
    tasks: list[dict[str, Any]],
    context_mode: str,
    ilo_bin: str,
) -> dict[str, tuple[list[str], str]]:
    """Resolve modules and joined text for each task before any LLM call."""
    prepared: dict[str, tuple[list[str], str]] = {}
    for task in tasks:
        mods = modules_for(context_mode, task)
        text = "\n\n".join(load_skill_text(m, ilo_bin) for m in mods)
        prepared[task["id"]] = (mods, text)
    return prepared


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


# Fixed exp-03 gloss. Keep this text stable across an A/B: the comparison
# is baseline repair (stderr + docs) versus this block on ILO-P003.
REPAIR_SHAPE_HINT = (
    "SHAPE: function headers are `name params>ret;body` — never `name()`.\n"
    "Zero-arg entry: `main>_;…` not `main()>_;…`. "
    "Example: `tri n:n>n;+n 1` then `main>_;prnt (tri 10)`.\n"
    "Call sites may use `(…)`; headers must not."
)


def repair_shape_hint_applies(error: str) -> bool:
    """True when a repair turn should carry the header-shape gloss.

    ILO-P003 is the mined paren-header trap. The same pair without the
    code token (expected greater-than, got left-paren) still counts, so a
    truncated diagnostic is kept.
    """
    if "ILO-P003" in error:
        return True
    expected_gt = (
        "expected `>`" in error
        or 'expected ">"' in error
        or "expected '>'" in error
        or "expected Greater" in error
    )
    got_paren = (
        "got `(`" in error
        or 'got "("' in error
        or "got '('" in error
        or "got LParen" in error
    )
    return expected_gt and got_paren


def make_repair_prompt(
    task: dict[str, Any],
    context: str,
    error: str,
    lang: str,
    previous_code: str = "",
    repair_shape_hint: bool = False,
) -> str:
    """Repair turn. Includes the program that failed (repair memory).

    repair_shape_hint appends REPAIR_SHAPE_HINT on an ilo turn whose error
    is ILO-P003 or the paren-header pair. Default off (baseline).
    """
    code = previous_code
    clipped = ""
    if len(code) > REPAIR_CODE_CHARS:
        code = code[:REPAIR_CODE_CHARS]
        clipped = "\n…[previous program truncated]\n"
    program = ""
    if code:
        program = f"Previous program:\n{code}{clipped}\n\n"
    hint = ""
    if (
        repair_shape_hint
        and lang == "ilo"
        and repair_shape_hint_applies(error)
    ):
        hint = f"\n{REPAIR_SHAPE_HINT}\n"
    return (
        f"The previous {lang} program for this task failed.\n"
        f"Task: {task['description']}\n"
        f"{program}"
        f"Error / actual output:\n{error}\n"
        f"{hint}\n"
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
# Provider selection
# ---------------------------------------------------------------------------

@dataclass(frozen=True)
class ModelSpec:
    """One model a live run will call.

    key is the CLI name recorded as the cell's model. model_id is the id
    sent on the wire. thinking is the DeepSeek thinking.type value, or
    None when the request omits the field.
    """

    key: str
    model_id: str
    thinking: str | None = None


def deepseek_key_present() -> bool:
    return bool(os.environ.get("DEEPSEEK_API_KEY", "").strip())


def deepseek_base_url(override: str | None = None) -> str:
    """Chat Completions origin, no trailing slash, no secret."""
    raw = override if override is not None and str(override).strip() else ""
    if not raw:
        raw = os.environ.get("DEEPSEEK_BASE_URL", "")
    if not str(raw).strip():
        raw = os.environ.get("DEEPSEEK_API_BASE", "")
    if not str(raw).strip():
        raw = DEFAULT_DEEPSEEK_BASE
    return str(raw).strip().rstrip("/")


def deepseek_chat_completions_url(base: str) -> str:
    """{base}/chat/completions, unless base already ends with that path."""
    trimmed = base.rstrip("/")
    if trimmed.endswith("/chat/completions"):
        return trimmed
    return trimmed + "/chat/completions"


def api_url_for(provider: str, base_url: str | None = None) -> str:
    if provider == "deepseek":
        return deepseek_chat_completions_url(deepseek_base_url(base_url))
    if provider == "anthropic":
        return ANTHROPIC_MESSAGES_URL
    raise ValueError(f"unknown provider {provider!r}")


def resolve_model_plan(
    provider_flag: str | None,
    model_flag: str | None,
    *,
    deepseek_key_set: bool,
) -> tuple[str, list[ModelSpec]]:
    """Choose the provider and the models a live run will call.

    An explicit DeepSeek model id selects DeepSeek. haiku, sonnet, and
    both select Anthropic when --provider is omitted (both is the
    Anthropic pair). With neither flag, a set DEEPSEEK_API_KEY selects
    DeepSeek and the single model deepseek-chat. Otherwise the Anthropic
    pair is unchanged.
    """
    provider = provider_flag
    model = model_flag

    if model is not None and provider is None:
        if model in DEEPSEEK_MODEL_PLAN or model.startswith("deepseek-"):
            provider = "deepseek"
        elif model in ANTHROPIC_MODEL_KEYS or model == "both":
            provider = "anthropic"
        else:
            raise ValueError(
                f"unknown model {model!r}. "
                "Anthropic: haiku, sonnet, both. "
                "DeepSeek: deepseek-chat, deepseek-reasoner, deepseek-flash, "
                "deepseek-v4-pro, or another deepseek-* id."
            )

    if provider is None:
        provider = "deepseek" if deepseek_key_set else "anthropic"
    if provider not in PROVIDERS:
        raise ValueError(f"unknown provider {provider!r}")

    if provider == "anthropic":
        if model is None or model == "both":
            keys = list(ANTHROPIC_MODEL_KEYS)
        elif model in MODELS:
            keys = [model]
        else:
            raise ValueError(
                f"model {model!r} is not an Anthropic model "
                "(haiku, sonnet, both). Pass --provider deepseek for a "
                "DeepSeek model."
            )
        return provider, [ModelSpec(key, MODELS[key], None) for key in keys]

    if model is None:
        keys = [DEFAULT_DEEPSEEK_MODEL]
    elif model == "both":
        keys = ["deepseek-chat", "deepseek-reasoner"]
    elif model in ANTHROPIC_MODEL_KEYS:
        raise ValueError(
            f"model {model!r} is an Anthropic model. "
            "Pass --provider anthropic, or a DeepSeek model id."
        )
    else:
        keys = [model]

    specs: list[ModelSpec] = []
    for key in keys:
        planned = DEEPSEEK_MODEL_PLAN.get(key)
        if planned is not None:
            wire, thinking = planned
            specs.append(ModelSpec(key, wire, thinking))
            continue
        if key.startswith("deepseek-"):
            specs.append(ModelSpec(key, key, None))
            continue
        raise ValueError(
            f"unknown DeepSeek model {key!r}. "
            "Use deepseek-chat, deepseek-reasoner, deepseek-flash, "
            "deepseek-v4-pro, or another deepseek-* id."
        )
    return provider, specs


def format_model_plan(specs: list[ModelSpec]) -> str:
    parts: list[str] = []
    for spec in specs:
        if spec.thinking:
            parts.append(f"{spec.key}={spec.model_id} thinking={spec.thinking}")
        elif spec.key == spec.model_id:
            parts.append(spec.key)
        else:
            parts.append(f"{spec.key}={spec.model_id}")
    return " ".join(parts)


def plan_lines(
    provider: str,
    specs: list[ModelSpec],
    base_url: str | None = None,
) -> list[str]:
    """Stdout lines for the resolved API. No secrets."""
    lines = [
        f"provider: {provider}",
        f"api: {api_url_for(provider, base_url)}",
        "models: " + format_model_plan(specs),
    ]
    translated = [
        spec for spec in specs
        if spec.key != spec.model_id or spec.thinking is not None
    ]
    if provider == "deepseek" and translated:
        bits = []
        for spec in translated:
            think = (
                f" thinking.type={spec.thinking}" if spec.thinking else ""
            )
            bits.append(f"{spec.key} -> model {spec.model_id}{think}")
        lines.append(
            "note: deepseek-chat and deepseek-reasoner are CLI names. "
            "The Chat Completions body sends deepseek-flash. DeepSeek "
            "discontinued those model ids on 2026-07-24 (changelog "
            "2026-04-24): chat is thinking disabled, reasoner is thinking "
            "enabled. " + "; ".join(bits)
        )
    if provider == "deepseek":
        lines.append(
            "note: generation_tokens is usage.completion_tokens. "
            "thinking_tokens is usage.completion_tokens_details."
            "reasoning_tokens when that value is a non-negative integer; "
            "otherwise null. generated_chars is the length of message "
            "content after fence stripping."
        )
    return lines


def api_key_for(provider: str) -> str | None:
    """Return the live key, or None after printing a fixed error line."""
    if provider == "deepseek":
        key = os.environ.get("DEEPSEEK_API_KEY", "").strip()
        if not key:
            print("ERROR: DEEPSEEK_API_KEY not set", file=sys.stderr)
            return None
        return key
    key = os.environ.get("ANTHROPIC_API_KEY", "").strip()
    if not key:
        print("ERROR: ANTHROPIC_API_KEY not set", file=sys.stderr)
        print(
            "       A live run uses DeepSeek when DEEPSEEK_API_KEY is set "
            "(--provider deepseek).",
            file=sys.stderr,
        )
        return None
    return key


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
        "max_tokens": ANTHROPIC_MAX_TOKENS,
        "system": system,
        "messages": [{"role": "user", "content": user}],
    }).encode()

    req = urllib.request.Request(
        ANTHROPIC_MESSAGES_URL,
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


def extract_openai_reasoning_tokens(usage: dict[str, Any]) -> int | None:
    """Reasoning-token split, or None when this payload has no integer split.

    A present 0 is a measured zero. The key reasoning_tokens, when present,
    wins even if the value is null: that is unknown, and a sibling
    thinking_tokens field is not a fallback. reasoning_content length is
    never a token count.
    """
    details = usage.get("completion_tokens_details")
    if isinstance(details, dict) and "reasoning_tokens" in details:
        return _nonneg_int(details.get("reasoning_tokens"))
    return extract_thinking_tokens(usage)


def openai_message_text(body: dict[str, Any]) -> str:
    """Programme text from the first choice. Reasoning is not programme text."""
    choices = body.get("choices")
    if not isinstance(choices, list) or not choices:
        return ""
    choice = choices[0]
    if not isinstance(choice, dict):
        return ""
    message = choice.get("message")
    if isinstance(message, str):
        return message
    if not isinstance(message, dict):
        return ""
    content = message.get("content")
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts: list[str] = []
        for block in content:
            if isinstance(block, str):
                parts.append(block)
                continue
            if not isinstance(block, dict):
                continue
            kind = block.get("type")
            if kind in ("thinking", "reasoning", "redacted_thinking"):
                continue
            text = block.get("text")
            if not isinstance(text, str):
                text = block.get("content") if kind in ("text", None) else None
            if isinstance(text, str):
                parts.append(text)
        return "\n".join(parts)
    return ""


def _openai_input_split(usage: dict[str, Any]) -> tuple[int, int]:
    """Return (uncached input, cache hit). Cache creation is not invented.

    When both prompt_cache_miss_tokens and prompt_cache_hit_tokens are
    non-negative integers, those are the split. prompt_tokens equals their
    sum on DeepSeek and is not added again. Otherwise prompt_tokens (or
    input_tokens) is the total, and prompt_tokens_details.cached_tokens is
    removed from that total when it is an integer that does not exceed it.
    """
    hit = _nonneg_int(usage.get("prompt_cache_hit_tokens"))
    miss = _nonneg_int(usage.get("prompt_cache_miss_tokens"))
    if hit is not None and miss is not None:
        return miss, hit
    prompt = _nonneg_int(usage.get("prompt_tokens"))
    if prompt is None:
        prompt = _nonneg_int(usage.get("input_tokens"))
    if prompt is None:
        prompt = 0
    cached: int | None = None
    details = usage.get("prompt_tokens_details")
    if isinstance(details, dict) and "cached_tokens" in details:
        cached = _nonneg_int(details.get("cached_tokens"))
    if cached is not None and cached <= prompt:
        return prompt - cached, cached
    return prompt, 0


def openai_finish_reason(body: dict[str, Any]) -> str | None:
    choices = body.get("choices")
    if not isinstance(choices, list) or not choices:
        return None
    choice = choices[0]
    if not isinstance(choice, dict):
        return None
    finish = choice.get("finish_reason")
    if finish is None:
        return None
    if not isinstance(finish, str):
        return str(finish)
    return finish


def parse_openai_chat_turn(body: dict[str, Any]) -> dict[str, Any]:
    """Normalise one Chat Completions body into the parse_provider_turn shape.

    generation_tokens is completion_tokens, then output_tokens. A missing
    usage field is 0, the same rule as the Messages parser. thinking_tokens
    stays null unless an integer reasoning split is present. Programme text
    is message content only; reasoning_content is ignored. served_model is
    the response model id, never the id we requested.
    """
    usage = body.get("usage")
    if not isinstance(usage, dict):
        usage = {}
    model = body.get("model")
    if not isinstance(model, str) or not model.strip():
        model = None
    generation = _nonneg_int(usage.get("completion_tokens"))
    if generation is None:
        generation = _nonneg_int(usage.get("output_tokens"))
    uncached, cache_hit = _openai_input_split(usage)
    return {
        "code": strip_fences(openai_message_text(body)),
        "generation_tokens": 0 if generation is None else generation,
        "input_tokens": uncached,
        "thinking_tokens": extract_openai_reasoning_tokens(usage),
        "served_model": model,
        "cache_hit_tokens": cache_hit,
        "cache_creation_tokens": 0,
        "finish_reason": openai_finish_reason(body),
    }


def build_deepseek_body(
    system: str,
    user: str,
    model_id: str,
    thinking: str | None,
    max_tokens: int = DEEPSEEK_MAX_TOKENS,
) -> dict[str, Any]:
    """JSON body for one non-streaming Chat Completions call."""
    if thinking is not None and thinking not in ("enabled", "disabled"):
        raise ValueError(f"thinking must be enabled or disabled, not {thinking!r}")
    body: dict[str, Any] = {
        "model": model_id,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
        "max_tokens": max_tokens,
        "stream": False,
    }
    if thinking is not None:
        body["thinking"] = {"type": thinking}
    return body


def call_deepseek(
    system: str,
    user: str,
    model_id: str,
    api_key: str,
    thinking: str | None = None,
    base_url: str | None = None,
    urlopen: Any = None,
) -> dict[str, Any]:
    """One DeepSeek Chat Completions turn.

    POST {base}/chat/completions with Authorization: Bearer. Parsed by
    parse_openai_chat_turn. Raises on transport or HTTP error. Does not
    invent a thinking split. The requested model_id is not copied into
    served_model.
    """
    import urllib.error
    import urllib.request

    url = deepseek_chat_completions_url(deepseek_base_url(base_url))
    payload = build_deepseek_body(system, user, model_id, thinking)
    data = json.dumps(payload).encode()
    req = urllib.request.Request(
        url,
        data=data,
        headers={
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json",
        },
        method="POST",
    )
    opener = urlopen or urllib.request.urlopen
    try:
        with opener(req, timeout=DEEPSEEK_TIMEOUT_S) as resp:
            raw = resp.read()
    except urllib.error.HTTPError as exc:
        detail = exc.read().decode("utf-8", errors="replace")[:500]
        raise RuntimeError(f"HTTP {exc.code} from {url}: {detail}") from exc
    try:
        body = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise ValueError(f"Chat Completions response from {url} was not JSON") from exc
    if not isinstance(body, dict):
        raise ValueError(f"Chat Completions response from {url} was not a JSON object")
    if body.get("error") and not body.get("choices"):
        raise RuntimeError(f"Chat Completions error from {url}: {body.get('error')}")
    return parse_openai_chat_turn(body)


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
    context_mode: str = DEFAULT_CONTEXT,
    ilo_modules: list[str] | None = None,
    ilo_context_text: str | None = None,
    lang2_docs: str | None = None,
    task_class: str | None = None,
    *,
    provider: str = "anthropic",
    thinking: str | None = None,
    base_url: str | None = None,
    repair_shape_hint: bool = False,
) -> dict[str, Any]:
    model_id_used = model_id
    is_ilo = (lang == "ilo")

    # Build context. The arm label is recorded on every cell, including the
    # second language, so a matrix stays attributable to the arm that ran.
    # ilo always gets skill text. A comparator gets --lang2-docs or the
    # memorised-prior stub (not a fair bakeoff).
    if is_ilo:
        if ilo_modules is None:
            context_modules = modules_for(context_mode, task)
        else:
            context_modules = list(ilo_modules)
        if ilo_context_text is None:
            context = "\n\n".join(
                load_skill_text(m, ilo_bin) for m in context_modules
            )
        else:
            context = ilo_context_text
        docs_source = "skills"
        fair_docs = True
        system = ILO_SYSTEM
        run_fn = lambda code: run_ilo(code, ilo_bin)  # noqa: E731
    else:
        context, docs_source, fair_docs, docs_err = lang2_documentation(
            lang, lang2_docs,
        )
        if docs_err:
            raise ValueError(docs_err)
        context_modules = []
        system = LANG2_SYSTEM.format(lang_name=lang)
        run_fn = lambda code: run_lang2(code, lang2_bin or "", lang2_ext)  # noqa: E731

    user = make_initial_prompt(task, context, lang)

    observations: list[AttemptObs] = []
    outcome = "failed"
    wall_start = time.monotonic()

    for attempt in range(1, retry_cap + 1):
        try:
            if provider == "deepseek":
                parsed = call_deepseek(
                    system, user, model_id, api_key,
                    thinking=thinking, base_url=base_url,
                )
            else:
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
        user = make_repair_prompt(
            task, context, error_detail, lang, code,
            repair_shape_hint=repair_shape_hint,
        )

    wall_time = time.monotonic() - wall_start
    cell = assemble_cell(
        task_id=task["id"],
        lang=lang,
        model_key=model_key,
        model_id=model_id_used,
        attempts=observations,
        wall_time_s=wall_time,
        final_outcome=outcome,
    )
    cell["context"] = context_mode
    cell["context_modules"] = list(context_modules)
    cell["lang_arm"] = lang
    cell["docs_source"] = docs_source
    cell["fair_docs"] = fair_docs
    if task_class is not None:
        cell["task_class"] = task_class
    cell["provider"] = provider
    cell["api"] = api_url_for(provider, base_url)
    if thinking is not None:
        cell["thinking_request"] = thinking
    cell["repair_shape_hint"] = bool(repair_shape_hint)
    return cell


# ---------------------------------------------------------------------------
# Output helpers
# ---------------------------------------------------------------------------

def write_json(
    results: list[dict[str, Any]],
    date_str: str,
    context_mode: str | None = None,
    *,
    leg: str | None = None,
    meta: dict[str, Any] | None = None,
) -> Path:
    out = BENCH_DIR / f"{result_stem(date_str, leg)}.json"
    payload: dict[str, Any] = {
        "generated": datetime.now(timezone.utc).isoformat(),
        "harness": "closed-loop-bench.py",
        "ticket": "ILO-364",
        "context": context_mode,
        "leg": leg,
        "niche": NICHE_STANCE,
        "note": METRIC_NOTE,
    }
    if meta:
        payload.update(meta)
    payload["results"] = results
    payload["context"] = context_mode
    payload["leg"] = leg
    payload["niche"] = NICHE_STANCE
    payload["note"] = METRIC_NOTE
    out.write_text(json.dumps(payload, indent=2) + "\n")
    return out


def write_markdown(
    results: list[dict[str, Any]],
    date_str: str,
    context_mode: str | None = None,
    *,
    leg: str | None = None,
) -> Path:
    out = BENCH_DIR / f"{result_stem(date_str, leg)}.md"

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
        versus = "no comparator"
    lines: list[str] = [
        f"# Closed-loop benchmark: ilo vs {versus}",
        "",
        f"Generated: {datetime.now(timezone.utc).strftime('%Y-%m-%d %H:%M UTC')}  ",
        "Ticket: [ILO-364](https://linear.app/ilo-lang/issue/ILO-364)  ",
        *([f"Context arm: {context_mode}  "] if context_mode else []),
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
                modules = ", ".join(r.get("context_modules") or []) or "(none)"
                unknown = int(r.get("thinking_unknown_attempts") or 0)
                lines += [
                    "",
                    f"**{lang} / {model}**  ",
                    f"- Language arm: {r.get('lang_arm', lang)}  ",
                    f"- Task class: {r.get('task_class', '-')}  ",
                    f"- Docs: {r.get('docs_source', '-')} "
                    f"(fair_docs={r.get('fair_docs', '-')})  ",
                    f"- Context arm: {r.get('context', context_mode)}  ",
                    f"- Context modules: {modules}  ",
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
                    f"- Provider: {r.get('provider', '-')}  ",
                ]
                if r.get("api"):
                    lines.append(f"- API: {r['api']}  ")
                if r.get("thinking_request"):
                    lines.append(
                        f"- Thinking request: {r['thinking_request']}  "
                    )
                if "repair_shape_hint" in r:
                    state = "on" if r["repair_shape_hint"] else "off"
                    lines.append(f"- Repair shape hint: {state}  ")
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
        f"- Context arm for this file: `{context_mode}`. "
        "Each cell also records `context` and `context_modules`.",
        "- Skill documentation is loaded once per process (steady-state caching).",
        "- Repair turns include the previous program.",
        "- One-shot economics (first attempt only) can be derived from `repair_tokens_by_turn` in the JSON.",
        f"- {NICHE_STANCE}.",
        "- Each cell's language arm is `lang_arm` (same value as `language`).",
        "- `language_neutral` on the JSON envelope is false when task text "
        "names a language. That confounds every arm.",
        "- Python arm: `--python` (python / python3 / .py). "
        "Bash arm: `--bash`, or `--lang2-name bash --lang2-bin bash --lang2-ext .sh`. "
        "The runner is `[binary, tempfile]`, so `bash file.sh` and `python3 file.py` both work.",
        "- A comparator file is suffixed with the leg "
        "(`closed-loop-<date>-python.json`) so interleaved arms do not clobber.",
        "- Re-run at any time; output files are date-stamped.",
        "- Human-floor bash programs: `bench/closed-loop/references-bash/`.",
    ]
    if any(r.get("provider") == "deepseek" for r in results):
        lines.append(
            "- DeepSeek cells are OpenAI Chat Completions. "
            "generation_tokens is usage.completion_tokens. "
            "thinking_tokens is usage.completion_tokens_details.reasoning_tokens "
            "when that value is a non-negative integer; otherwise null. "
            "reasoning_content is not programme text and is not a token count. "
            "generated_chars is the length of message content after fence stripping. "
            "CLI names deepseek-chat and deepseek-reasoner are sent as "
            "deepseek-flash with thinking disabled or enabled."
        )
    stub_arms = sorted({
        r.get("lang_arm", r["language"])
        for r in results
        if r.get("docs_source") == "stub"
    })
    ops_tasks = sorted({
        r["task"] for r in results if r.get("task_class") == "ops"
    })
    if stub_arms:
        lines.append(
            "- " + ", ".join(stub_arms) + " used the memorised-prior documentation "
            "stub. This is not a fair bakeoff against ilo skills. "
            "Pass `--lang2-docs PATH`."
        )
    else:
        lines.append(
            "- Comparator documentation was loaded from `--lang2-docs`, "
            "or this run had no comparator arm."
        )
    if ops_tasks:
        lines.append(
            "- Ops tasks (" + ", ".join(ops_tasks) + ") are out of niche. "
            "A bash win on those rows, including wall time, is not an ilo failure."
        )
    lines += [
        "",
        "## Deferred",
        "",
        "- Zero CLI integration (ILO-364 Phase 5.b): blocked on Zero being installable in CI.",
        "  `python3 scripts/closed-loop-bench.py --lang2-name zero --lang2-bin <path-to-zero> --lang2-ext .zero`",
        "- Pre/post [ILO-360](https://linear.app/ilo-lang/issue/ILO-360) comparison: run baseline now, re-run after typed fix plans land.",
        "- Empirical retry-cap tuning: first run curves are in `repair_tokens_by_turn`; "
        "adjust `--retry-cap` once flattening point is visible.",
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
    parser.add_argument(
        "--provider",
        choices=list(PROVIDERS),
        default=None,
        help=(
            "LLM API. deepseek posts OpenAI-compatible Chat Completions to "
            "DEEPSEEK_BASE_URL (default https://api.deepseek.com/chat/completions). "
            "anthropic posts the Messages API. "
            "Default: deepseek when DEEPSEEK_API_KEY is set, otherwise anthropic. "
            "An explicit DeepSeek model id selects deepseek. haiku, sonnet, and "
            "both select anthropic when --provider is omitted."
        ),
    )
    parser.add_argument(
        "--model",
        default=None,
        help=(
            "Model key. Anthropic: haiku, sonnet, both (default both when "
            "Anthropic is selected). DeepSeek: deepseek-chat (default when "
            "DeepSeek is selected; non-thinking flash), deepseek-reasoner "
            "(thinking flash), deepseek-flash, deepseek-v4-pro, or another "
            "deepseek-* id sent unchanged. "
            "deepseek-chat and deepseek-reasoner are CLI names; the request "
            "sends model deepseek-flash with thinking disabled or enabled, "
            "because DeepSeek discontinued those model ids on 2026-07-24. "
            "--model both with --provider deepseek runs chat and reasoner."
        ),
    )
    parser.add_argument("--task", metavar="ID",
                        help="Run only this task ID.")
    parser.add_argument("--lang2-name", default=None,
                        help="Name of the comparator arm (e.g. zero, python, bash).")
    parser.add_argument("--lang2-bin", default=None,
                        help="Path to the comparator CLI. The runner executes [bin, file].")
    parser.add_argument("--lang2-ext", default=None,
                        help="Source extension. Default: .py for python, .sh for bash, else .zero.")
    parser.add_argument("--python", action="store_true",
                        help="Comparator shorthand: name python, binary python3, extension .py. "
                             "--lang2-bin and --lang2-ext override.")
    parser.add_argument("--bash", action="store_true",
                        help="Comparator shorthand: name bash, binary bash, extension .sh. "
                             "Same arm as --lang2-name bash --lang2-bin bash --lang2-ext .sh.")
    parser.add_argument("--lang2-docs", default=None,
                        help="Documentation file for the comparator, sent where ilo sends "
                             "skill text. Without it the arm is a memorised-prior stub and "
                             "is not a fair bakeoff.")
    parser.add_argument("--require-language-neutral", action="store_true",
                        help="Refuse to start if a task description names a language. "
                             "The default is a warning; current tasks name ilo.")
    parser.add_argument(
        "--context",
        choices=list(CONTEXT_MODES),
        default=None,
        help=(
            "ilo skill modules to load. "
            "core = ilo-language + ilo-builtins-core. "
            "curated = core + ilo-builtins-sig (default). "
            "full = curated + io + text + math (explicit balloon). "
            "task-modules = task['modules'] from tasks.json "
            "(error if that list is missing)."
        ),
    )
    parser.add_argument(
        "--modules-from-task",
        action="store_true",
        help="Alias for --context task-modules.",
    )
    parser.add_argument("--dry-run", action="store_true",
                        help="Print task specs and context arm, then exit. "
                             "No LLM calls and no API key.")
    parser.add_argument(
        "--repair-shape-hint",
        action="store_true",
        help=(
            "On an ilo repair turn whose stderr contains ILO-P003, or the "
            "paren-header pair (expected `>`, got `(`), append a fixed "
            "function-header gloss inside the repair prompt: "
            "`name params>ret;body`, `main>_;…` against `main()>…`, and "
            "one correct one-liner (`tri n:n>n;+n 1` then "
            "`main>_;prnt (tri 10)`). Default off (baseline repair text). "
            "The initial prompt is unchanged. Manifesto P6: structured "
            "compiler-to-agent surface, aimed at cutting retries."
        ),
    )
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

    try:
        provider, specs = resolve_model_plan(
            args.provider,
            args.model,
            deepseek_key_set=deepseek_key_present(),
        )
    except ValueError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2
    base_url = deepseek_base_url() if provider == "deepseek" else None

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

    lang2, lang2_err = resolve_lang2(
        args.python, args.bash, args.lang2_name, args.lang2_bin, args.lang2_ext,
    )
    if lang2_err:
        print(f"ERROR: {lang2_err}", file=sys.stderr)
        return 2

    # Load tasks
    tasks_data = json.loads(TASKS_FILE.read_text())
    all_tasks = tasks_data["tasks"]
    if args.task:
        all_tasks = [t for t in all_tasks if t["id"] == args.task]
        if not all_tasks:
            print(f"ERROR: task '{args.task}' not found in tasks.json", file=sys.stderr)
            return 2

    try:
        classes = load_task_classes()
        for task in all_tasks:
            task_class_of(task, classes)
        context_mode = resolve_context_mode(args.context, args.modules_from_task)
        prepared = prepare_ilo_contexts(all_tasks, context_mode, args.ilo)
    except (ContextError, ValueError) as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2

    named = check_language_neutral(
        all_tasks, [lang2.name] if lang2 else [],
    )
    if named:
        print(
            "WARNING: task description names a language: " + ", ".join(named),
            file=sys.stderr,
        )
        print(
            "         Descriptions are sent verbatim to every arm, so this "
            "confounds the bakeoff. Pass --require-language-neutral to refuse.",
            file=sys.stderr,
        )
        if args.require_language_neutral:
            return 2

    docs_source = "skills"
    fair_docs = True
    if lang2:
        _text, docs_source, fair_docs, docs_err = lang2_documentation(
            lang2.name, args.lang2_docs,
        )
        if docs_err:
            print(f"ERROR: {docs_err}", file=sys.stderr)
            return 2
        if not fair_docs:
            print(
                f"WARNING: {lang2.name} docs_source=stub. Not a fair bakeoff "
                "against ilo skills. Pass --lang2-docs PATH.",
                file=sys.stderr,
            )
    elif args.lang2_docs:
        print(
            "WARNING: --lang2-docs has no effect without a comparator arm.",
            file=sys.stderr,
        )

    if args.dry_run:
        arms = ["ilo"] + ([lang2.name] if lang2 else [])
        print("arms: " + " ".join(arms))
        print(
            f"lang_arm: ilo bin={args.ilo} ext=.ilo "
            "docs_source=skills fair_docs=true"
        )
        if lang2:
            ok, detail = probe_lang_bin(lang2.bin)
            print(
                f"lang_arm: {lang2.name} bin={lang2.bin} ext={lang2.ext} "
                f"docs_source={docs_source} fair_docs={str(fair_docs).lower()} "
                f"probe={'ok' if ok else 'fail'}"
            )
            print(f"probe_detail: {detail}")
            if fair_docs:
                print(
                    f"note: {lang2.name} documentation loaded from {args.lang2_docs}."
                )
            else:
                print(
                    "note: lang2 docs are the memorised-prior stub; this is not "
                    "a fair bakeoff against ilo skills. Pass --lang2-docs PATH."
                )
        print(f"context: {context_mode}")
        print(
            "repair_shape_hint: "
            + ("on" if args.repair_shape_hint else "off")
        )
        if args.repair_shape_hint:
            print(
                "note: ilo repair turns with ILO-P003 (or expected `>` / "
                "got `(`) append a fixed header-shape gloss."
            )
        print(f"niche: {NICHE_STANCE}")
        for line in plan_lines(provider, specs, base_url):
            print(line)
        print("language_neutral: " + ("false" if named else "true"))
        if named:
            print(
                "note: task text names a language (" + ", ".join(named) + "). "
                "That confounds every arm."
            )
        print("Tasks:")
        for t in all_tasks:
            mods, text = prepared[t["id"]]
            klass = task_class_of(t, classes) or "-"
            print(
                f"  [{t['id']}] class={klass} {t['description'][:80]}..."
            )
            print(f"          expected: {t['expected_output']!r}")
            print(f"          context: {context_mode}")
            print(f"          modules: {' '.join(mods)}")
            print(f"          context_chars: {len(text)}")
        print()
        print(HOWTO_FIXTURE, end="")
        return 0

    api_key = api_key_for(provider)
    if not api_key:
        return 2

    # Verify ilo
    try:
        subprocess.run([args.ilo, "--version"], capture_output=True, check=True, timeout=5)
    except (FileNotFoundError, subprocess.CalledProcessError, subprocess.TimeoutExpired):
        print(f"ERROR: ilo binary not found or not runnable: {args.ilo}", file=sys.stderr)
        return 2

    # A requested comparator that cannot run must not be relabelled as ilo-only.
    if lang2:
        probe_ok, probe_detail = probe_lang_bin(lang2.bin)
        if not probe_ok:
            print(
                f"ERROR: {lang2.name} binary not usable ({probe_detail}).",
                file=sys.stderr,
            )
            return 2
        print(f"lang2 probe: {probe_detail}", file=sys.stderr)

    # ilo is always the language-of-record arm.
    languages = ["ilo"]
    if lang2:
        languages.append(lang2.name)

    total_runs = len(all_tasks) * len(languages) * len(specs)
    for line in plan_lines(provider, specs, base_url):
        print(line)
    print(
        f"Closed-loop bench: {len(all_tasks)} tasks × "
        f"{len(languages)} languages ({' '.join(languages)}) × "
        f"{len(specs)} models = "
        f"{total_runs} runs  (retry_cap={args.retry_cap} context={context_mode} "
        f"docs_source={docs_source} fair_docs={fair_docs} provider={provider} "
        f"repair_shape_hint={'on' if args.repair_shape_hint else 'off'})"
    )

    results: list[dict[str, Any]] = []
    run_num = 0

    for task in all_tasks:
        for lang in languages:
            for spec in specs:
                run_num += 1
                print(
                    f"\n[{run_num}/{total_runs}] task={task['id']} "
                    f"lang={lang} model={spec.key} provider={provider}",
                    file=sys.stderr,
                )
                mods, text = prepared[task["id"]]
                r = run_task(
                    task=task,
                    lang=lang,
                    model_key=spec.key,
                    model_id=spec.model_id,
                    api_key=api_key,
                    retry_cap=args.retry_cap,
                    ilo_bin=args.ilo,
                    lang2_bin=lang2.bin if lang2 else None,
                    lang2_ext=lang2.ext if lang2 else ".zero",
                    context_mode=context_mode,
                    ilo_modules=mods,
                    ilo_context_text=text,
                    lang2_docs=args.lang2_docs,
                    task_class=task_class_of(task, classes),
                    provider=provider,
                    thinking=spec.thinking,
                    base_url=base_url,
                    repair_shape_hint=args.repair_shape_hint,
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
    leg = lang2.name if lang2 else None
    meta = {
        "lang_arms": languages,
        "lang2_docs": args.lang2_docs,
        "docs_source": docs_source,
        "fair_bakeoff": bool(lang2) and fair_docs and not named,
        "language_neutral": not named,
        "language_named_in": named,
        "provider": provider,
        "api": api_url_for(provider, base_url),
        "repair_shape_hint": bool(args.repair_shape_hint),
        "models": [
            {
                "model": spec.key,
                "model_id": spec.model_id,
                "thinking_request": spec.thinking,
            }
            for spec in specs
        ],
    }
    json_path = write_json(results, date_str, context_mode, leg=leg, meta=meta)
    md_path = write_markdown(results, date_str, context_mode, leg=leg)

    print("\nResults written:")
    print(f"  JSON: {json_path}")
    print(f"  MD:   {md_path}")

    # Summary table to stdout. Thinking and chars lead; the provider
    # output sum is not a column.
    print("\nSummary (thinking and emitted chars; provider output is in the JSON):")
    print(
        f"{'task':<22} {'lang':<6} {'model':<18} "
        f"{'think':>8} {'chars':>7} {'attempts':>8} {'outcome':<8} {'time':>6}"
    )
    print("-" * 92)
    for r in results:
        att = str(r["attempts_to_success"]) if r["attempts_to_success"] else "-"
        think = format_thinking(r["thinking_tokens"], r["thinking_unknown_attempts"])
        print(
            f"{r['task']:<22} {r['language']:<6} {r['model']:<18} "
            f"{think:>8} {r['generated_chars']:>7} {att:>8} {r['final_outcome']:<8} "
            f"{r['wall_time_s']:>5.1f}s"
        )

    success_count = sum(1 for r in results if r["final_outcome"] == "working")
    print(f"\n{success_count}/{total_runs} runs succeeded.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
