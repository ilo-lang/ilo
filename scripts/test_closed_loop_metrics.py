#!/usr/bin/env python3
"""Honest-column checks for closed-loop-bench. No API key and no ilo binary.

When bench/metric-schema.json and scripts/validate-closed-loop-results.py
are both present (PR #797), the committed fixture is also checked with
that validator.
"""

from __future__ import annotations

import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SCRIPT = REPO_ROOT / "scripts" / "closed-loop-bench.py"
FIXTURE = REPO_ROOT / "bench" / "fixtures" / "closed-loop-harness-shape.json"
VALIDATOR = REPO_ROOT / "scripts" / "validate-closed-loop-results.py"
SCHEMA = REPO_ROOT / "bench" / "metric-schema.json"

BASE_REQUIRED = [
    "task", "language", "model", "model_id",
    "generation_tokens", "input_tokens", "attempts_total",
    "success_rate", "wall_time_s", "final_outcome",
]
OUTCOMES = {"working", "partial", "failed"}


def load_harness():
    spec = importlib.util.spec_from_file_location("closed_loop_bench", SCRIPT)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {SCRIPT}")
    mod = importlib.util.module_from_spec(spec)
    # dataclass looks the class's module up in sys.modules during decoration.
    sys.modules[spec.name] = mod
    spec.loader.exec_module(mod)
    return mod


H = load_harness()


def pr0_cell_errors(cell: dict) -> list[str]:
    """Mirror the PR #797 cell rules this harness has to satisfy."""
    errs: list[str] = []
    if not isinstance(cell, dict):
        return ["cell is not an object"]
    for key in BASE_REQUIRED:
        if key not in cell:
            errs.append(f"missing {key}")
    if cell.get("final_outcome") not in OUTCOMES and "final_outcome" in cell:
        errs.append(f"bad final_outcome={cell.get('final_outcome')!r}")
    thinking = cell.get("thinking_tokens", None)
    chars = cell.get("generated_chars", None)
    thinking_ok = "thinking_tokens" in cell and isinstance(thinking, int) and not isinstance(thinking, bool) and thinking >= 0
    chars_ok = "generated_chars" in cell and isinstance(chars, int) and not isinstance(chars, bool) and chars >= 0
    if not thinking_ok and not chars_ok:
        errs.append("not honest: need non-null thinking_tokens or generated_chars")
    unknown = cell.get("thinking_unknown_attempts")
    if isinstance(unknown, int) and not isinstance(unknown, bool) and unknown >= 1:
        if cell.get("code_tokens") is not None:
            errs.append("code_tokens set while thinking is unknown")
    for key in ("generation_tokens", "input_tokens", "attempts_total"):
        value = cell.get(key)
        if key in cell and (isinstance(value, bool) or not isinstance(value, int) or value < 0):
            errs.append(f"{key} must be an integer >= 0")
    rate = cell.get("success_rate")
    if "success_rate" in cell and (
        isinstance(rate, bool) or not isinstance(rate, (int, float)) or not 0 <= rate <= 1
    ):
        errs.append("success_rate out of range")
    wall = cell.get("wall_time_s")
    if "wall_time_s" in cell and (
        isinstance(wall, bool) or not isinstance(wall, (int, float)) or wall < 0
    ):
        errs.append("wall_time_s out of range")
    return errs


class ParseTests(unittest.TestCase):
    def test_thinking_block_is_not_code_and_fences_are_stripped(self) -> None:
        parsed = H.parse_provider_turn({
            "model": "claude-haiku-4-5-20251001",
            "stop_reason": "end_turn",
            "content": [
                {"type": "thinking", "thinking": "long thought that is not code"},
                {"type": "redacted_thinking", "data": "secret"},
                {"type": "text", "text": "```ilo\n-- run: main\n```"},
            ],
            "usage": {
                "input_tokens": 10,
                "output_tokens": 50,
                "output_tokens_details": {"thinking_tokens": 40},
                "cache_read_input_tokens": 3,
                "cache_creation_input_tokens": 2,
            },
        })
        self.assertEqual(parsed["code"], "-- run: main")
        self.assertNotIn("long thought", parsed["code"])
        self.assertEqual(parsed["thinking_tokens"], 40)
        self.assertEqual(parsed["generation_tokens"], 50)
        self.assertEqual(parsed["input_tokens"], 10)
        self.assertEqual(parsed["cache_hit_tokens"], 3)
        self.assertEqual(parsed["cache_creation_tokens"], 2)
        self.assertEqual(parsed["served_model"], "claude-haiku-4-5-20251001")
        self.assertEqual(len(parsed["code"]), 12)

    def test_missing_split_is_unknown_not_zero(self) -> None:
        parsed = H.parse_provider_turn({
            "content": [{"type": "text", "text": "abc"}],
            "usage": {"input_tokens": 1, "output_tokens": 2},
        })
        self.assertIsNone(parsed["thinking_tokens"])
        self.assertEqual(parsed["code"], "abc")
        self.assertIsNone(parsed["served_model"])

    def test_zero_thinking_is_a_measurement(self) -> None:
        self.assertEqual(
            H.extract_thinking_tokens({
                "output_tokens_details": {"thinking_tokens": 0},
            }),
            0,
        )

    def test_null_string_and_bool_are_unknown(self) -> None:
        self.assertIsNone(H.extract_thinking_tokens({
            "output_tokens_details": {"thinking_tokens": None},
        }))
        self.assertIsNone(H.extract_thinking_tokens({
            "output_tokens_details": {"thinking_tokens": "90"},
        }))
        self.assertIsNone(H.extract_thinking_tokens({
            "output_tokens_details": {"thinking_tokens": True},
        }))
        self.assertIsNone(H.extract_thinking_tokens({
            "output_tokens_details": {"thinking_tokens": -1},
        }))

    def test_explicit_null_does_not_fall_through(self) -> None:
        self.assertIsNone(H.extract_thinking_tokens({
            "output_tokens_details": {"thinking_tokens": None},
            "thinking_tokens": 5,
        }))

    def test_flat_thinking_field_when_details_absent(self) -> None:
        self.assertEqual(H.extract_thinking_tokens({"thinking_tokens": 12}), 12)

    def test_strip_fences_leaves_plain_programme(self) -> None:
        self.assertEqual(H.strip_fences("FIRST"), "FIRST")
        self.assertEqual(H.strip_fences("```ilo\nabc\n```"), "abc")


class AssembleTests(unittest.TestCase):
    def test_full_split_sets_code_tokens(self) -> None:
        cell = H.assemble_cell(
            task_id="simple-function",
            lang="ilo",
            model_key="haiku",
            model_id="claude-haiku-4-5",
            attempts=[H.AttemptObs(
                code="abcd",
                generation_tokens=120,
                input_tokens=10,
                thinking_tokens=90,
                served_model="claude-haiku-4-5",
                cache_hit_tokens=3,
                cache_creation_tokens=2,
                outcome="working",
            )],
            wall_time_s=1.25,
            final_outcome="working",
        )
        self.assertEqual(cell["thinking_tokens"], 90)
        self.assertEqual(cell["thinking_unknown_attempts"], 0)
        self.assertEqual(cell["code_tokens"], 30)
        self.assertEqual(cell["generation_tokens"], 120)
        self.assertEqual(cell["generated_chars"], 4)
        self.assertEqual(cell["final_code_chars"], 4)
        self.assertEqual(cell["code_chars_by_turn"], [4])
        self.assertEqual(cell["input_cache_hit_tokens"], 3)
        self.assertEqual(cell["input_cache_miss_tokens"], 12)
        self.assertEqual(cell["input_tokens"], 15)
        self.assertEqual(cell["served_models"], ["claude-haiku-4-5"])
        self.assertEqual(len(cell["attempt_trace"]), 1)
        self.assertEqual(pr0_cell_errors(cell), [])

    def test_known_zero_thinking_is_not_unknown(self) -> None:
        cell = H.assemble_cell(
            task_id="t",
            lang="ilo",
            model_key="haiku",
            model_id="m",
            attempts=[H.AttemptObs(
                code="z",
                generation_tokens=8,
                input_tokens=1,
                thinking_tokens=0,
                served_model="m",
                outcome="failed",
            )],
            wall_time_s=0.1,
            final_outcome="failed",
        )
        self.assertEqual(cell["thinking_tokens"], 0)
        self.assertEqual(cell["thinking_unknown_attempts"], 0)
        self.assertEqual(cell["code_tokens"], 8)

    def test_any_unknown_nulls_code_tokens_and_keeps_lower_bound(self) -> None:
        cell = H.assemble_cell(
            task_id="t",
            lang="ilo",
            model_key="haiku",
            model_id="m",
            attempts=[
                H.AttemptObs(
                    code="aa",
                    generation_tokens=20,
                    input_tokens=5,
                    thinking_tokens=None,
                    served_model="m-a",
                    outcome="failed",
                ),
                H.AttemptObs(
                    code="bbb",
                    generation_tokens=11,
                    input_tokens=6,
                    thinking_tokens=7,
                    served_model="m-b",
                    outcome="working",
                ),
            ],
            wall_time_s=0.2,
            final_outcome="working",
        )
        self.assertEqual(cell["thinking_tokens"], 7)
        self.assertEqual(cell["thinking_unknown_attempts"], 1)
        self.assertIsNone(cell["code_tokens"])
        self.assertEqual(cell["generated_chars"], 5)
        self.assertEqual(cell["code_chars_by_turn"], [2, 3])
        self.assertEqual(cell["generation_tokens"], 31)
        self.assertEqual(cell["served_models"], ["m-a", "m-b"])
        self.assertEqual(cell["repair_tokens_by_turn"], [11])
        self.assertEqual(pr0_cell_errors(cell), [])

    def test_all_unknown_thinking_is_null_not_zero(self) -> None:
        cell = H.assemble_cell(
            task_id="t",
            lang="python",
            model_key="haiku",
            model_id="m",
            attempts=[H.AttemptObs(
                code="hi",
                generation_tokens=4,
                input_tokens=1,
                thinking_tokens=None,
                served_model=None,
                outcome="partial",
            )],
            wall_time_s=0.2,
            final_outcome="partial",
        )
        self.assertIsNone(cell["thinking_tokens"])
        self.assertEqual(cell["thinking_unknown_attempts"], 1)
        self.assertIsNone(cell["code_tokens"])
        self.assertEqual(cell["generated_chars"], 2)
        self.assertEqual(cell["served_models"], [])
        self.assertEqual(pr0_cell_errors(cell), [])

    def test_transport_failure_is_not_an_unknown_thinking_attempt(self) -> None:
        cell = H.assemble_cell(
            task_id="t",
            lang="ilo",
            model_key="haiku",
            model_id="m",
            attempts=[
                H.AttemptObs(
                    code="",
                    generation_tokens=0,
                    input_tokens=0,
                    thinking_tokens=None,
                    served_model=None,
                    billed=False,
                    api_error="down",
                ),
                H.AttemptObs(
                    code="OK",
                    generation_tokens=9,
                    input_tokens=3,
                    thinking_tokens=3,
                    served_model="served",
                    outcome="working",
                ),
            ],
            wall_time_s=0.4,
            final_outcome="working",
        )
        self.assertEqual(cell["thinking_unknown_attempts"], 0)
        self.assertEqual(cell["thinking_tokens"], 3)
        self.assertEqual(cell["code_tokens"], 6)
        self.assertEqual(cell["generated_chars"], 2)
        self.assertEqual(cell["attempts_total"], 2)
        self.assertEqual(cell["attempts_to_success"], 2)
        self.assertFalse(cell["attempt_trace"][0]["billed"])
        self.assertEqual(cell["attempt_trace"][0]["api_error"], "down")
        self.assertEqual(cell["served_models"], ["served"])
        self.assertEqual(pr0_cell_errors(cell), [])


class LoopTests(unittest.TestCase):
    def setUp(self) -> None:
        self._call = H.call_llm
        self._ilo = H.run_ilo
        self._sleep = H.time.sleep
        H.time.sleep = lambda *_a, **_k: None

    def tearDown(self) -> None:
        H.call_llm = self._call
        H.run_ilo = self._ilo
        H.time.sleep = self._sleep

    def test_loop_strips_fences_remembers_code_and_nulls_code_tokens(self) -> None:
        prompts: list[str] = []
        seen: list[str] = []

        def fake_llm(system: str, user: str, model_id: str, api_key: str) -> dict:
            prompts.append(user)
            if len(prompts) == 1:
                return H.parse_provider_turn({
                    "model": "claude-haiku-4-5",
                    "stop_reason": "end_turn",
                    "content": [
                        {"type": "thinking", "thinking": "not code"},
                        {"type": "text", "text": "```ilo\nFIRST\n```"},
                    ],
                    "usage": {"input_tokens": 100, "output_tokens": 20},
                })
            return H.parse_provider_turn({
                "model": "claude-haiku-4-5-20251001",
                "stop_reason": "end_turn",
                "content": [{"type": "text", "text": "FIXED"}],
                "usage": {
                    "input_tokens": 50,
                    "output_tokens": 11,
                    "output_tokens_details": {"thinking_tokens": 7},
                    "cache_read_input_tokens": 4,
                    "cache_creation_input_tokens": 1,
                },
            })

        def fake_ilo(code: str, ilo_bin: str) -> tuple[str, str, int]:
            seen.append(code)
            if code == "FIXED":
                return "ok", "", 0
            return "", "nope", 1

        H.call_llm = fake_llm
        H.run_ilo = fake_ilo
        cell = H.run_task(
            {"id": "simple-function", "description": "add", "expected_output": "ok"},
            "ilo", "haiku", "claude-haiku-4-5", "test-key", 3,
            "ilo", None, ".zero",
        )
        self.assertEqual(seen, ["FIRST", "FIXED"])
        self.assertIn("FIRST", prompts[1])
        self.assertIn("nope", prompts[1])
        self.assertEqual(cell["generated_chars"], len("FIRST") + len("FIXED"))
        self.assertEqual(cell["code_chars_by_turn"], [5, 5])
        self.assertEqual(cell["thinking_tokens"], 7)
        self.assertEqual(cell["thinking_unknown_attempts"], 1)
        self.assertIsNone(cell["code_tokens"])
        self.assertEqual(cell["generation_tokens"], 31)
        self.assertEqual(cell["input_tokens"], 155)
        self.assertEqual(cell["input_cache_hit_tokens"], 4)
        self.assertEqual(cell["input_cache_miss_tokens"], 151)
        self.assertEqual(
            cell["served_models"],
            ["claude-haiku-4-5", "claude-haiku-4-5-20251001"],
        )
        self.assertEqual(cell["final_outcome"], "working")
        self.assertEqual(cell["attempt_trace"][0]["code"], "FIRST")
        self.assertIsNone(cell["attempt_trace"][0]["thinking_tokens"])
        self.assertEqual(cell["attempt_trace"][1]["thinking_tokens"], 7)
        self.assertNotIn("not code", cell["attempt_trace"][0]["code"])
        self.assertEqual(pr0_cell_errors(cell), [])

    def test_transport_error_does_not_void_a_later_split(self) -> None:
        calls = {"n": 0}

        def fake_llm(system: str, user: str, model_id: str, api_key: str) -> dict:
            calls["n"] += 1
            if calls["n"] == 1:
                raise RuntimeError("down")
            return H.parse_provider_turn({
                "model": "served-id",
                "stop_reason": "end_turn",
                "content": [{"type": "text", "text": "OK"}],
                "usage": {
                    "input_tokens": 3,
                    "output_tokens": 9,
                    "output_tokens_details": {"thinking_tokens": 3},
                },
            })

        H.call_llm = fake_llm
        H.run_ilo = lambda code, ilo_bin: ("ok", "", 0)
        cell = H.run_task(
            {"id": "simple-function", "description": "add", "expected_output": "ok"},
            "ilo", "haiku", "claude-haiku-4-5", "test-key", 3,
            "ilo", None, ".zero",
        )
        self.assertEqual(cell["thinking_unknown_attempts"], 0)
        self.assertEqual(cell["code_tokens"], 6)
        self.assertEqual(cell["generated_chars"], 2)
        self.assertEqual(cell["attempts_total"], 2)
        self.assertEqual(cell["served_models"], ["served-id"])
        self.assertEqual(cell["model_id"], "claude-haiku-4-5")
        self.assertEqual(pr0_cell_errors(cell), [])


class ReportTests(unittest.TestCase):
    def test_markdown_leads_with_thinking_and_chars(self) -> None:
        cell = H.fixture_payload()["results"][0]
        previous = H.BENCH_DIR
        try:
            with tempfile.TemporaryDirectory() as tmp:
                H.BENCH_DIR = Path(tmp)
                path = H.write_markdown([cell], "fixture")
                text = path.read_text()
        finally:
            H.BENCH_DIR = previous
        header = next(line for line in text.splitlines() if line.startswith("| task"))
        self.assertIn("think", header)
        self.assertIn("chars", header)
        self.assertNotIn("gen", header)
        self.assertNotIn("generation_tokens", header)
        self.assertIn("not a density claim", text)
        self.assertIn("Thinking tokens:", text)
        self.assertIn("Emitted code chars:", text)
        self.assertIn("Provider output tokens (generation_tokens, not a density claim):", text)

    def test_undivided_generation_tokens_fail_the_cell_rule(self) -> None:
        undivided = {
            "task": "simple-function",
            "language": "ilo",
            "model": "haiku",
            "model_id": "m",
            "generation_tokens": 100,
            "input_tokens": 10,
            "attempts_total": 1,
            "success_rate": 0,
            "wall_time_s": 0.1,
            "final_outcome": "failed",
        }
        self.assertTrue(pr0_cell_errors(undivided))
        dishonest = dict(undivided)
        dishonest.update({
            "thinking_tokens": 40,
            "thinking_unknown_attempts": 2,
            "code_tokens": 60,
            "generated_chars": 20,
        })
        self.assertTrue(any("code_tokens" in err for err in pr0_cell_errors(dishonest)))

    def test_fixture_payload_passes_and_matches_the_file(self) -> None:
        payload = H.fixture_payload()
        self.assertEqual(json.loads(FIXTURE.read_text()), payload)
        self.assertIn("Not a measurement", payload["note"])
        self.assertIn("not a density claim", payload["note"])
        cells = payload["results"]
        self.assertEqual(len(cells), 2)
        for cell in cells:
            self.assertEqual(pr0_cell_errors(cell), [])
            self.assertIn("thinking_tokens", cell)
            self.assertIn("generated_chars", cell)
            self.assertIn("attempt_trace", cell)
            self.assertIn("served_models", cell)
        split, mixed = cells
        self.assertEqual(split["thinking_unknown_attempts"], 0)
        self.assertEqual(split["code_tokens"], split["generation_tokens"] - split["thinking_tokens"])
        self.assertGreater(mixed["thinking_unknown_attempts"], 0)
        self.assertIsNone(mixed["code_tokens"])
        self.assertIsInstance(mixed["thinking_tokens"], int)
        self.assertIsInstance(mixed["generated_chars"], int)

    def test_dry_run_and_emit_fixture_need_no_api_key(self) -> None:
        env = os.environ.copy()
        env.pop("ANTHROPIC_API_KEY", None)
        dry = subprocess.run(
            [sys.executable, str(SCRIPT), "--dry-run", "--task", "simple-function"],
            cwd=REPO_ROOT,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(dry.returncode, 0, dry.stderr)
        self.assertIn("simple-function", dry.stdout)
        self.assertIn("validate-closed-loop-results.py", dry.stdout)
        self.assertNotIn("ANTHROPIC_API_KEY not set", dry.stderr)

        with tempfile.TemporaryDirectory() as tmp:
            dest = Path(tmp) / "cell.json"
            emitted = subprocess.run(
                [sys.executable, str(SCRIPT), "--emit-fixture", str(dest)],
                cwd=REPO_ROOT,
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(emitted.returncode, 0, emitted.stderr)
            self.assertEqual(json.loads(dest.read_text()), H.fixture_payload())
            self.assertIn("not a measurement", emitted.stdout)

    def test_official_validator_when_present(self) -> None:
        if not (VALIDATOR.exists() and SCHEMA.exists()):
            self.skipTest("metric schema and validator land in PR #797")
        env = os.environ.copy()
        env.pop("ANTHROPIC_API_KEY", None)
        good = subprocess.run(
            [sys.executable, str(VALIDATOR), str(FIXTURE)],
            cwd=REPO_ROOT,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(good.returncode, 0, good.stdout + good.stderr)
        undivided = {
            "results": [{
                "task": "simple-function",
                "language": "ilo",
                "model": "haiku",
                "model_id": "m",
                "generation_tokens": 10,
                "input_tokens": 10,
                "attempts_total": 1,
                "success_rate": 0,
                "wall_time_s": 0.1,
                "final_outcome": "failed",
            }],
        }
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "undivided.json"
            path.write_text(json.dumps(undivided))
            bad = subprocess.run(
                [sys.executable, str(VALIDATOR), str(path)],
                cwd=REPO_ROOT,
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )
        self.assertEqual(bad.returncode, 1, bad.stdout + bad.stderr)


if __name__ == "__main__":
    unittest.main()
