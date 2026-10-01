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

  # Python arm (name python, binary python3, extension .py)
  python3 scripts/closed-loop-bench.py --python

  # bash arm. The runner is `bash <file>.sh` (the same [bin, file] shape).
  python3 scripts/closed-loop-bench.py --bash
  python3 scripts/closed-loop-bench.py --lang2-name bash --lang2-bin bash --lang2-ext .sh

  # fair comparator docs (without this, lang2 is a memorised-prior stub)
  python3 scripts/closed-loop-bench.py --python --lang2-docs path/to/python.md

  # dry-run selects the arm and does not call an API (no key required)
  python3 scripts/closed-loop-bench.py --dry-run
  python3 scripts/closed-loop-bench.py --dry-run --python

  # single task
  python3 scripts/closed-loop-bench.py --task simple-function

  # single model
  python3 scripts/closed-loop-bench.py --model haiku

Environment
  ANTHROPIC_API_KEY   required for a live run. Not used by --dry-run.

Language arms
  ilo is always included. --python / --bash / --lang2-* add one comparator.
  Result cells label that arm (`language` and `lang_arm`). A comparator run
  writes closed-loop-<date>-<leg>.json so python and bash do not clobber.
  task_class (artefact | ops | sanity) is joined from
  bench/closed-loop/task-class.json. An ops row, or a bash win on wall time,
  is outside the language-of-record niche and is not an ilo manifesto loss.
  Human-floor scripts: bench/closed-loop/references-bash/.

Retry cap
  Default N=5.  Override with --retry-cap N.
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


def dry_run_lines(
    tasks: list[dict[str, Any]],
    classes: dict[str, str],
    lang2: Lang2 | None,
    docs_path: str | None,
    ilo_bin: str,
    named: list[str],
) -> list[str]:
    """Text for --dry-run. No API call. Probe failure does not raise."""
    arms = ["ilo"] + ([lang2.name] if lang2 else [])
    lines = [
        "arms: " + " ".join(arms),
        f"lang_arm: ilo bin={ilo_bin} ext=.ilo docs_source=skills fair_docs=true",
        f"niche: {NICHE_STANCE}",
    ]
    if lang2:
        _text, src, fair, _err = lang2_documentation(lang2.name, docs_path)
        ok, detail = probe_lang_bin(lang2.bin)
        lines.append(
            f"lang_arm: {lang2.name} bin={lang2.bin} ext={lang2.ext} "
            f"docs_source={src} fair_docs={str(fair).lower()} "
            f"probe={'ok' if ok else 'fail'}"
        )
        lines.append(f"probe_detail: {detail}")
        if fair:
            lines.append(
                f"note: {lang2.name} documentation loaded from {docs_path}."
            )
        else:
            lines.append(
                "note: lang2 docs are the memorised-prior stub; this is not "
                "a fair bakeoff against ilo skills. Pass --lang2-docs PATH."
            )
    lines.append(
        "language_neutral: " + ("false" if named else "true")
    )
    if named:
        lines.append(
            "note: task text names a language (" + ", ".join(named) + "). "
            "That confounds every arm."
        )
    lines.append("Tasks:")
    for task in tasks:
        klass = task_class_of(task, classes) or "-"
        lines.append(
            f"  [{task['id']}] class={klass} {task['description'][:80]}..."
        )
        lines.append(f"          expected: {task['expected_output']!r}")
    return lines


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
    lang: str,                # "ilo" or lang2_name — the language arm
    model_key: str,           # "haiku" | "sonnet"
    model_id: str,
    api_key: str,
    retry_cap: int,
    ilo_bin: str,
    lang2_bin: str | None,
    lang2_ext: str,
    lang2_docs: str | None = None,
    task_class: str | None = None,
) -> dict[str, Any]:
    model_id_used = model_id
    is_ilo = (lang == "ilo")

    # Build context. ilo always gets skill text. A comparator gets
    # --lang2-docs or the memorised-prior stub (not a fair bakeoff).
    if is_ilo:
        context = ilo_context(ilo_bin)
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

    cell: dict[str, Any] = {
        "task": task["id"],
        "language": lang,
        "lang_arm": lang,
        "model": model_key,
        "model_id": model_id_used,
        "docs_source": docs_source,
        "fair_docs": fair_docs,
        "generation_tokens": total_gen_tokens,
        "input_tokens": total_input_tokens,
        "repair_tokens_by_turn": repair_tokens_by_turn,
        "attempts_to_success": attempts_to_success,
        "attempts_total": attempts,
        "success_rate": 1.0 if outcome == "working" else 0.0,
        "wall_time_s": round(wall_time, 2),
        "final_outcome": outcome,
    }
    if task_class is not None:
        cell["task_class"] = task_class
    return cell


# ---------------------------------------------------------------------------
# Output helpers
# ---------------------------------------------------------------------------

def write_json(
    results: list[dict[str, Any]],
    date_str: str,
    leg: str | None = None,
    meta: dict[str, Any] | None = None,
) -> Path:
    out = BENCH_DIR / f"{result_stem(date_str, leg)}.json"
    payload: dict[str, Any] = {
        "generated": datetime.now(timezone.utc).isoformat(),
        "harness": "closed-loop-bench.py",
        "ticket": "ILO-364",
        "leg": leg,
        "niche": NICHE_STANCE,
    }
    if meta:
        payload.update(meta)
    payload["results"] = results
    out.write_text(json.dumps(payload, indent=2))
    return out


def write_markdown(
    results: list[dict[str, Any]],
    date_str: str,
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
                    f"- Language arm: {r.get('lang_arm', lang)}  ",
                    f"- Task class: {r.get('task_class', '-')}  ",
                    f"- Docs: {r.get('docs_source', '-')} "
                    f"(fair_docs={r.get('fair_docs', '-')})  ",
                    f"- Generation tokens: {r['generation_tokens']}  ",
                    f"- Input tokens (context): {r['input_tokens']}  ",
                    f"- Attempts total: {r['attempts_total']}  ",
                    f"- Attempts to success: {r['attempts_to_success']}  ",
                    f"- Repair tokens by turn: {r['repair_tokens_by_turn']}  ",
                    f"- Wall time: {r['wall_time_s']}s  ",
                    f"- Outcome: **{r['final_outcome']}**  ",
                ]
        lines.append("")

    stub_arms = sorted({
        r.get("lang_arm", r["language"])
        for r in results
        if r.get("docs_source") == "stub"
    })
    ops_tasks = sorted({
        r["task"] for r in results if r.get("task_class") == "ops"
    })
    lines += [
        f"## Notes",
        f"",
        f"- {NICHE_STANCE}.",
        f"- Each cell's language arm is `lang_arm` (same value as `language`).",
        "- `language_neutral` on the JSON envelope is false when task text "
        "names a language. That confounds every arm.",
        f"- Python arm: `--python` (python / python3 / .py). "
        f"Bash arm: `--bash`, or `--lang2-name bash --lang2-bin bash --lang2-ext .sh`. "
        f"The runner is `[binary, tempfile]`, so `bash file.sh` and `python3 file.py` both work.",
        f"- A comparator file is suffixed with the leg "
        f"(`closed-loop-<date>-python.json`) so interleaved arms do not clobber.",
    ]
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
        f"- Other CLIs (Zero and the rest) still use "
        f"`--lang2-name NAME --lang2-bin BIN --lang2-ext .ext`.",
        f"- Skill documentation is loaded once per process (steady-state caching).",
        f"- One-shot economics (first attempt only) can be derived from `repair_tokens_by_turn` in the JSON.",
        f"- Re-run at any time; output files are date-stamped.",
        f"- Human-floor bash programs: `bench/closed-loop/references-bash/`.",
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
    parser.add_argument("--dry-run", action="store_true",
                        help="Print the selected arms and task specs. No LLM calls, no API key.")
    parser.add_argument("--output-dir", default=None,
                        help="Override output directory (default: bench/).")
    args = parser.parse_args()

    global BENCH_DIR
    if args.output_dir:
        BENCH_DIR = Path(args.output_dir)

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
    except ValueError as exc:
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
        for line in dry_run_lines(
            all_tasks, classes, lang2, args.lang2_docs, args.ilo, named,
        ):
            print(line)
        return 0

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

    api_key = os.environ.get("ANTHROPIC_API_KEY", "")
    if not api_key:
        print("ERROR: ANTHROPIC_API_KEY not set", file=sys.stderr)
        return 2

    # Determine models to run
    if args.model == "both":
        model_keys = list(MODELS.keys())
    else:
        model_keys = [args.model]

    # Determine languages. ilo is always the language-of-record arm.
    languages = ["ilo"]
    if lang2:
        languages.append(lang2.name)

    total_runs = len(all_tasks) * len(languages) * len(model_keys)
    print(
        f"Closed-loop bench: {len(all_tasks)} tasks × "
        f"{len(languages)} languages ({' '.join(languages)}) × "
        f"{len(model_keys)} models = "
        f"{total_runs} runs  (retry_cap={args.retry_cap}, "
        f"docs_source={docs_source}, fair_docs={fair_docs})"
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
                    lang2_bin=lang2.bin if lang2 else None,
                    lang2_ext=lang2.ext if lang2 else ".zero",
                    lang2_docs=args.lang2_docs,
                    task_class=task_class_of(task, classes),
                )
                results.append(r)
                print(
                    f"  -> outcome={r['final_outcome']} "
                    f"gen_tokens={r['generation_tokens']} "
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
    }
    json_path = write_json(results, date_str, leg=leg, meta=meta)
    md_path = write_markdown(results, date_str, leg=leg)

    print(f"\nResults written:")
    print(f"  JSON: {json_path}")
    print(f"  MD:   {md_path}")

    # Summary table to stdout
    print("\nSummary:")
    print(
        f"{'task':<22} {'arm':<8} {'class':<9} {'model':<6} "
        f"{'gen_tok':>7} {'attempts':>8} {'outcome':<8} {'time':>6}"
    )
    print("-" * 86)
    for r in results:
        att = str(r["attempts_to_success"]) if r["attempts_to_success"] else "-"
        print(
            f"{r['task']:<22} {r.get('lang_arm', r['language']):<8} "
            f"{r.get('task_class') or '-':<9} {r['model']:<6} "
            f"{r['generation_tokens']:>7} {att:>8} {r['final_outcome']:<8} "
            f"{r['wall_time_s']:>5.1f}s"
        )

    success_count = sum(1 for r in results if r["final_outcome"] == "working")
    print(f"\n{success_count}/{total_runs} runs succeeded.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
