#!/usr/bin/env python3
"""Validate closed-loop result JSON against the honest spine schema (stdlib).

Exit 0 if every cell has the schema's required fields and at least one of
thinking_tokens or generated_chars (null does not count). Exit 1 on failure.
Exit 2 on usage or schema errors. Prints a short report.

generation_tokens alone is not a density claim. The main harness is still
pre-repair; see bench/MANIFESTO-METRIC.md. This script does not call a model
and does not read ANTHROPIC_API_KEY.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

# Canonical honest columns. The schema's allOf/anyOf must name exactly these;
# a schema edit that drops the union fails closed instead of accepting
# undivided generation_tokens.
HONEST_COLUMNS = ("thinking_tokens", "generated_chars")
REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_SCHEMA = REPO_ROOT / "bench" / "metric-schema.json"


def load_rules(path: Path) -> tuple[list[str], set[str]]:
    try:
        schema = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        raise SystemExit(f"schema {path}: {exc}") from exc
    try:
        required = list(schema["required"])
        outcomes = set(schema["properties"]["final_outcome"]["enum"])
        honest: list[str] = []
        for clause in schema.get("allOf", []):
            for alt in clause.get("anyOf", []):
                honest.extend(alt.get("required", []))
    except (KeyError, TypeError) as exc:
        raise SystemExit(f"schema {path}: unreadable contract ({exc})") from exc
    if set(honest) != set(HONEST_COLUMNS) or len(honest) != len(HONEST_COLUMNS):
        raise SystemExit(
            f"schema {path}: honest anyOf must require thinking_tokens or "
            f"generated_chars, got {honest}"
        )
    if not required:
        raise SystemExit(f"schema {path}: empty required list")
    if not outcomes:
        raise SystemExit(f"schema {path}: empty final_outcome enum")
    return required, outcomes


def validate_cell(
    cell: object, idx: int, required: list[str], outcomes: set[str]
) -> list[tuple[str, bool]]:
    """Return (message, historical) pairs.

    historical is true only for the undivided-column gap that
    --allow-historical-fail may tolerate. Every other defect is hard.
    """
    errs: list[tuple[str, bool]] = []
    if not isinstance(cell, dict):
        return [(f"[{idx}] cell is not an object", False)]
    for key in required:
        if key not in cell:
            errs.append((f"[{idx}] missing {key}", False))
    if cell.get("final_outcome") not in outcomes and "final_outcome" in cell:
        errs.append(
            (f"[{idx}] bad final_outcome={cell.get('final_outcome')!r}", False)
        )
    if not any(key in cell and cell[key] is not None for key in HONEST_COLUMNS):
        errs.append(
            (
                f"[{idx}] not honest: need thinking_tokens or generated_chars "
                f"(undivided generation_tokens is not a density claim)",
                True,
            )
        )
    # Null rule: unknown thinking must not be folded into code_tokens.
    unk = cell.get("thinking_unknown_attempts")
    if unk and cell.get("code_tokens") is not None:
        errs.append(
            (
                f"[{idx}] code_tokens set while thinking_unknown_attempts={unk} "
                f"(must be null — do not fold unknown thinking into code)",
                False,
            )
        )
    return errs


def load_results(path: Path) -> tuple[list, str | None]:
    try:
        data = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as exc:
        return [], f"invalid JSON ({exc})"
    if isinstance(data, dict):
        if "results" not in data:
            return [], "missing results array"
        results = data["results"]
    elif isinstance(data, list):
        results = data
    else:
        return [], "expected an object with results or a list of cells"
    if not isinstance(results, list):
        return [], "results is not a list"
    return results, None


def validate_file(
    path: Path,
    required: list[str],
    outcomes: set[str],
    allow_historical_fail: bool,
) -> int:
    """Print a report. Return the number of hard errors (0 means pass)."""
    results, err = load_results(path)
    print(f"== {path} ==")
    if err is not None:
        print(f"FAIL ({err})")
        return 1
    n_cells = len(results)
    print(f"({n_cells} {'cell' if n_cells == 1 else 'cells'})")
    file_errs: list[tuple[str, bool]] = []
    for i, cell in enumerate(results):
        file_errs.extend(validate_cell(cell, i, required, outcomes))
    if not file_errs:
        print("OK (base + honest column present)")
        return 0

    hard = [msg for msg, historical in file_errs if not historical]
    historical = [msg for msg, historical in file_errs if historical]
    tolerated = allow_historical_fail and not hard
    label = "HISTORICAL" if tolerated else "FAIL"
    n_issues = len(file_errs)
    print(f"{label} ({n_issues} {'issue' if n_issues == 1 else 'issues'})")
    shown = (hard + historical)[:40]
    for msg in shown:
        print(f"  {msg}")
    hidden = len(file_errs) - len(shown)
    if hidden > 0:
        print(f"  … +{hidden} more")
    if tolerated:
        print(
            "tolerated: undivided generation_tokens is not a density claim "
            "(--allow-historical-fail)"
        )
        return 0
    return len(hard) if hard else len(historical)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Validate closed-loop result JSON. generation_tokens alone is "
            "not a density claim."
        )
    )
    parser.add_argument(
        "paths",
        nargs="+",
        type=Path,
        help="closed-loop result JSON files (object with results, or a list)",
    )
    parser.add_argument(
        "--schema",
        type=Path,
        default=DEFAULT_SCHEMA,
        help=f"metric schema (default: {DEFAULT_SCHEMA})",
    )
    parser.add_argument(
        "--allow-historical-fail",
        action="store_true",
        help=(
            "report cells that lack thinking_tokens and generated_chars, "
            "but exit 0 when that is their only defect"
        ),
    )
    args = parser.parse_args(argv)
    required, outcomes = load_rules(args.schema)
    hard_files = 0
    for path in args.paths:
        if validate_file(path, required, outcomes, args.allow_historical_fail):
            hard_files += 1
    return 1 if hard_files else 0


if __name__ == "__main__":
    sys.exit(main())
