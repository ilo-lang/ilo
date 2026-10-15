#!/usr/bin/env python3
"""Validate closed-loop result JSON against the honest spine schema (stdlib).

Schema: bench/metric-schema.json. This script is the executable gate; it
checks that its required fields, outcome enum, and honest-column names still
match that file.

Exit 0 if every cell has the required base fields and at least one non-null
honest column (thinking_tokens or generated_chars). Exit 1 on failure.
Exit 2 on usage errors. Prints a short report.

No API key. Does not run the harness.

Historical pre-repair results (paths under a `historical` directory, or a
basename starting with `closed-loop-2026-08-03`) still print FAIL. Pass
`--allow-historical-fail` to keep those failures from changing the exit
code, so a mixed run can require new results to pass. The flag does not
excuse any other file.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SCHEMA_PATH = REPO_ROOT / "bench" / "metric-schema.json"

BASE_REQUIRED = [
    "task", "language", "model", "model_id",
    "generation_tokens", "input_tokens", "attempts_total",
    "success_rate", "wall_time_s", "final_outcome",
]
HONEST_ANY = ("thinking_tokens", "generated_chars")
OUTCOMES = {"working", "partial", "failed"}
INT_FIELDS = {
    "generation_tokens": 0,
    "input_tokens": 0,
    "attempts_total": 0,
}


def load_schema(path: Path) -> dict:
    try:
        return json.loads(path.read_text())
    except FileNotFoundError:
        print(f"schema not found: {path}", file=sys.stderr)
        raise SystemExit(2)
    except json.JSONDecodeError as exc:
        print(f"schema is not JSON: {path}: {exc}", file=sys.stderr)
        raise SystemExit(2)


def assert_schema_alignment(schema: dict) -> None:
    """Fail closed if the executable checks drift from metric-schema.json."""
    if schema.get("required") != BASE_REQUIRED:
        print(
            "schema required drifted from validator BASE_REQUIRED:\n"
            f"  schema: {schema.get('required')}\n"
            f"  script: {BASE_REQUIRED}",
            file=sys.stderr,
        )
        raise SystemExit(2)
    outcome_enum = set(
        schema.get("properties", {}).get("final_outcome", {}).get("enum") or []
    )
    if outcome_enum != OUTCOMES:
        print(
            f"schema final_outcome enum {sorted(outcome_enum)} != {sorted(OUTCOMES)}",
            file=sys.stderr,
        )
        raise SystemExit(2)
    honest: list[str] = []
    saw_null_rule = False
    for clause in schema.get("allOf") or []:
        for branch in clause.get("anyOf") or []:
            req = branch.get("required") or []
            if len(req) != 1:
                print(
                    "honest anyOf branch must require exactly one field",
                    file=sys.stderr,
                )
                raise SystemExit(2)
            honest.append(req[0])
            prop = (branch.get("properties") or {}).get(req[0]) or {}
            if prop.get("type") != "integer":
                print(
                    f"honest column {req[0]} must be type integer in its anyOf "
                    "branch (null does not count)",
                    file=sys.stderr,
                )
                raise SystemExit(2)
        cond = clause.get("if") or {}
        then = clause.get("then") or {}
        if "thinking_unknown_attempts" in (cond.get("required") or []):
            code = (then.get("properties") or {}).get("code_tokens") or {}
            if code.get("type") == "null":
                saw_null_rule = True
    if tuple(honest) != HONEST_ANY:
        print(
            f"schema honest columns {honest} != {list(HONEST_ANY)}",
            file=sys.stderr,
        )
        raise SystemExit(2)
    if not saw_null_rule:
        print(
            "schema missing code_tokens null rule "
            "(thinking_unknown_attempts >= 1 ⇒ code_tokens is null)",
            file=sys.stderr,
        )
        raise SystemExit(2)


def is_historical(path: Path) -> bool:
    if "historical" in path.parts:
        return True
    return path.name.startswith("closed-loop-2026-08-03")


def _bad_int(value: object, minimum: int) -> bool:
    return isinstance(value, bool) or not isinstance(value, int) or value < minimum


def validate_cell(cell: dict, idx: int) -> list[str]:
    errs: list[str] = []
    if not isinstance(cell, dict):
        return [f"[{idx}] cell is not an object"]
    for k in BASE_REQUIRED:
        if k not in cell:
            errs.append(f"[{idx}] missing {k}")
    if cell.get("final_outcome") not in OUTCOMES and "final_outcome" in cell:
        errs.append(f"[{idx}] bad final_outcome={cell.get('final_outcome')!r}")
    if not any(k in cell and cell[k] is not None for k in HONEST_ANY):
        errs.append(
            f"[{idx}] not honest: need thinking_tokens or generated_chars "
            f"(undivided generation_tokens is not a density claim)"
        )
    # Null rule: unknown thinking must not be folded into code_tokens.
    unk = cell.get("thinking_unknown_attempts")
    if unk and cell.get("code_tokens") is not None:
        errs.append(
            f"[{idx}] code_tokens set while thinking_unknown_attempts={unk} "
            f"(must be null — do not fold unknown thinking into code)"
        )
    for key, minimum in INT_FIELDS.items():
        if key in cell and _bad_int(cell[key], minimum):
            errs.append(f"[{idx}] {key} must be an integer >= {minimum}")
    for key in HONEST_ANY:
        if key in cell and cell[key] is not None and _bad_int(cell[key], 0):
            errs.append(f"[{idx}] {key} must be an integer >= 0 or absent")
    if "success_rate" in cell:
        rate = cell["success_rate"]
        if (
            isinstance(rate, bool)
            or not isinstance(rate, (int, float))
            or not 0 <= rate <= 1
        ):
            errs.append(f"[{idx}] success_rate must be a number in [0, 1]")
    if "wall_time_s" in cell:
        wall = cell["wall_time_s"]
        if isinstance(wall, bool) or not isinstance(wall, (int, float)) or wall < 0:
            errs.append(f"[{idx}] wall_time_s must be a number >= 0")
    return errs


def _cell_count(path: Path) -> str:
    try:
        data = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError):
        return "unreadable"
    if isinstance(data, dict):
        results = data.get("results")
    elif isinstance(data, list):
        results = data
    else:
        return "unreadable"
    if not isinstance(results, list):
        return "unreadable"
    n = len(results)
    return "1 cell" if n == 1 else f"{n} cells"


def validate_file(path: Path) -> list[str]:
    try:
        data = json.loads(path.read_text())
    except FileNotFoundError:
        return [f"file not found: {path}"]
    except json.JSONDecodeError as exc:
        return [f"not JSON: {exc}"]
    if isinstance(data, dict):
        if "results" not in data:
            return ["missing results array"]
        results = data["results"]
    elif isinstance(data, list):
        results = data
    else:
        return ["top level must be an object with results, or an array of cells"]
    if not isinstance(results, list):
        return ["results is not an array"]
    errs: list[str] = []
    for i, cell in enumerate(results):
        errs.extend(validate_cell(cell, i))
    return errs


def main(argv: list[str]) -> int:
    allow_historical = False
    paths: list[str] = []
    for arg in argv[1:]:
        if arg == "--allow-historical-fail":
            allow_historical = True
        elif arg in ("-h", "--help"):
            print(
                "usage: validate-closed-loop-results.py "
                "[--allow-historical-fail] <closed-loop.json> [...]",
                file=sys.stderr,
            )
            return 2
        elif arg.startswith("-"):
            print(f"unknown flag: {arg}", file=sys.stderr)
            return 2
        else:
            paths.append(arg)
    if not paths:
        print(
            "usage: validate-closed-loop-results.py "
            "[--allow-historical-fail] <closed-loop.json> [...]",
            file=sys.stderr,
        )
        return 2

    assert_schema_alignment(load_schema(SCHEMA_PATH))

    blocking: list[str] = []
    for path_s in paths:
        path = Path(path_s)
        historical = is_historical(path)
        file_errs = validate_file(path)
        count = _cell_count(path)
        print(f"== {path}  ({count}) ==")
        if file_errs:
            tag = ""
            if historical:
                tag = (
                    " [historical; allowed]"
                    if allow_historical
                    else " [historical pre-repair; not a publishable result]"
                )
            noun = "issue" if len(file_errs) == 1 else "issues"
            print(f"FAIL ({len(file_errs)} {noun}){tag}")
            for e in file_errs[:40]:
                print(" ", e)
            if len(file_errs) > 40:
                print(f"  … +{len(file_errs) - 40} more")
            if not (historical and allow_historical):
                blocking.extend(file_errs)
        else:
            print("OK (base + honest column present)")
    return 1 if blocking else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
