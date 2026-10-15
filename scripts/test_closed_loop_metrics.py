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


class _Resp:
    def __init__(self, payload: dict) -> None:
        self._raw = json.dumps(payload).encode()

    def read(self) -> bytes:
        return self._raw

    def __enter__(self) -> "_Resp":
        return self

    def __exit__(self, *_exc: object) -> bool:
        return False


class DeepSeekTests(unittest.TestCase):
    def test_chat_alias_posts_flash_and_keeps_reasoning_out_of_the_programme(self) -> None:
        captured: dict = {}

        def fake_urlopen(req, timeout=0):  # noqa: ANN001
            captured["url"] = req.full_url
            captured["method"] = req.get_method()
            captured["auth"] = req.get_header("Authorization")
            captured["body"] = json.loads(req.data.decode())
            captured["timeout"] = timeout
            return _Resp({
                "model": "deepseek-flash",
                "choices": [{
                    "finish_reason": "stop",
                    "message": {
                        "role": "assistant",
                        "content": "```ilo\nprnt 1\n```",
                        "reasoning_content": "this is not the programme",
                    },
                }],
                "usage": {
                    "prompt_tokens": 30,
                    "completion_tokens": 12,
                    "prompt_cache_hit_tokens": 10,
                    "prompt_cache_miss_tokens": 20,
                },
            })

        provider, specs = H.resolve_model_plan(
            "deepseek", "deepseek-chat", deepseek_key_set=False,
        )
        self.assertEqual(provider, "deepseek")
        spec = specs[0]
        parsed = H.call_deepseek(
            "system", "user", spec.model_id, "secret-key",
            thinking=spec.thinking,
            base_url="https://api.deepseek.com",
            urlopen=fake_urlopen,
        )
        self.assertEqual(captured["url"], "https://api.deepseek.com/chat/completions")
        self.assertEqual(captured["method"], "POST")
        self.assertEqual(captured["auth"], "Bearer secret-key")
        self.assertEqual(captured["body"]["model"], "deepseek-flash")
        self.assertEqual(captured["body"]["thinking"], {"type": "disabled"})
        self.assertEqual(captured["body"]["stream"], False)
        self.assertEqual(captured["body"]["messages"][0]["role"], "system")
        self.assertEqual(captured["body"]["messages"][1]["role"], "user")
        self.assertEqual(parsed["code"], "prnt 1")
        self.assertNotIn("not the programme", parsed["code"])
        self.assertEqual(parsed["generation_tokens"], 12)
        self.assertIsNone(parsed["thinking_tokens"])
        self.assertEqual(parsed["input_tokens"], 20)
        self.assertEqual(parsed["cache_hit_tokens"], 10)
        self.assertEqual(parsed["served_model"], "deepseek-flash")
        self.assertEqual(parsed["finish_reason"], "stop")
        self.assertEqual(len(parsed["code"]), 6)

    def test_reasoning_tokens_are_the_split_and_zero_counts(self) -> None:
        parsed = H.parse_openai_chat_turn({
            "model": "deepseek-flash",
            "choices": [{
                "finish_reason": "stop",
                "message": {
                    "content": "CODE",
                    "reasoning_content": "x" * 500,
                },
            }],
            "usage": {
                "completion_tokens": 40,
                "prompt_tokens": 8,
                "completion_tokens_details": {"reasoning_tokens": 31},
            },
        })
        self.assertEqual(parsed["code"], "CODE")
        self.assertEqual(parsed["generation_tokens"], 40)
        self.assertEqual(parsed["thinking_tokens"], 31)
        self.assertEqual(parsed["input_tokens"], 8)
        self.assertEqual(
            H.extract_openai_reasoning_tokens({
                "completion_tokens_details": {"reasoning_tokens": 0},
                "thinking_tokens": 9,
            }),
            0,
        )

    def test_missing_or_null_split_stays_unknown(self) -> None:
        parsed = H.parse_openai_chat_turn({
            "choices": [{"message": {"content": "abc", "reasoning_content": "think"}}],
            "usage": {"completion_tokens": 3, "prompt_tokens": 1},
        })
        self.assertIsNone(parsed["thinking_tokens"])
        self.assertEqual(parsed["code"], "abc")
        self.assertEqual(parsed["generation_tokens"], 3)
        self.assertIsNone(H.extract_openai_reasoning_tokens({
            "completion_tokens_details": {"reasoning_tokens": None},
            "output_tokens_details": {"thinking_tokens": 5},
        }))
        self.assertIsNone(H.extract_openai_reasoning_tokens({
            "completion_tokens_details": {"reasoning_tokens": "31"},
        }))
        self.assertIsNone(H.extract_openai_reasoning_tokens({
            "completion_tokens_details": {"reasoning_tokens": True},
        }))

    def test_cached_tokens_are_not_added_twice(self) -> None:
        uncached, hit = H._openai_input_split({
            "prompt_tokens": 30,
            "prompt_tokens_details": {"cached_tokens": 10},
        })
        self.assertEqual((uncached, hit), (20, 10))
        uncached, hit = H._openai_input_split({"prompt_tokens": 30})
        self.assertEqual((uncached, hit), (30, 0))

    def test_base_url_joins_chat_completions(self) -> None:
        self.assertEqual(
            H.deepseek_chat_completions_url("https://api.deepseek.com"),
            "https://api.deepseek.com/chat/completions",
        )
        self.assertEqual(
            H.deepseek_chat_completions_url("https://api.deepseek.com/v1/"),
            "https://api.deepseek.com/v1/chat/completions",
        )
        self.assertEqual(
            H.deepseek_chat_completions_url("https://api.deepseek.com/chat/completions"),
            "https://api.deepseek.com/chat/completions",
        )
        previous = os.environ.get("DEEPSEEK_BASE_URL")
        alias = os.environ.get("DEEPSEEK_API_BASE")
        os.environ["DEEPSEEK_BASE_URL"] = "https://example.test/v1/"
        os.environ.pop("DEEPSEEK_API_BASE", None)
        try:
            self.assertEqual(
                H.api_url_for("deepseek"),
                "https://example.test/v1/chat/completions",
            )
        finally:
            if previous is None:
                os.environ.pop("DEEPSEEK_BASE_URL", None)
            else:
                os.environ["DEEPSEEK_BASE_URL"] = previous
            if alias is None:
                os.environ.pop("DEEPSEEK_API_BASE", None)
            else:
                os.environ["DEEPSEEK_API_BASE"] = alias

    def test_default_live_plan_prefers_deepseek_when_the_key_is_set(self) -> None:
        provider, specs = H.resolve_model_plan(None, None, deepseek_key_set=True)
        self.assertEqual(provider, "deepseek")
        self.assertEqual([spec.key for spec in specs], ["deepseek-chat"])
        self.assertEqual(specs[0].model_id, "deepseek-flash")
        self.assertEqual(specs[0].thinking, "disabled")

        provider, specs = H.resolve_model_plan(None, None, deepseek_key_set=False)
        self.assertEqual(provider, "anthropic")
        self.assertEqual([spec.key for spec in specs], ["haiku", "sonnet"])
        self.assertEqual(specs[0].model_id, "claude-haiku-4-5")

        provider, specs = H.resolve_model_plan(None, "haiku", deepseek_key_set=True)
        self.assertEqual(provider, "anthropic")
        self.assertEqual(specs[0].model_id, "claude-haiku-4-5")

        provider, specs = H.resolve_model_plan("deepseek", "both", deepseek_key_set=False)
        self.assertEqual(
            [(spec.key, spec.model_id, spec.thinking) for spec in specs],
            [
                ("deepseek-chat", "deepseek-flash", "disabled"),
                ("deepseek-reasoner", "deepseek-flash", "enabled"),
            ],
        )
        provider, specs = H.resolve_model_plan(
            "deepseek", "deepseek-v4-pro", deepseek_key_set=False,
        )
        self.assertEqual(specs[0].model_id, "deepseek-v4-pro")
        self.assertIsNone(specs[0].thinking)

        with self.assertRaises(ValueError):
            H.resolve_model_plan("anthropic", "deepseek-chat", deepseek_key_set=True)
        with self.assertRaises(ValueError):
            H.resolve_model_plan("deepseek", "haiku", deepseek_key_set=True)

    def test_http_error_names_the_chat_url_and_not_the_key(self) -> None:
        import io
        import urllib.error

        def boom(req, timeout=0):  # noqa: ANN001
            raise urllib.error.HTTPError(
                req.full_url, 401, "unauthorized", hdrs=None,
                fp=io.BytesIO(b'{"error":"invalid"}'),
            )

        with self.assertRaises(RuntimeError) as ctx:
            H.call_deepseek(
                "s", "u", "deepseek-flash", "super-secret-key",
                urlopen=boom, base_url="https://api.deepseek.com",
            )
        message = str(ctx.exception)
        self.assertIn("401", message)
        self.assertIn("https://api.deepseek.com/chat/completions", message)
        self.assertNotIn("super-secret-key", message)

    def test_run_task_calls_deepseek_and_keeps_chars_when_thinking_is_null(self) -> None:
        calls: list[tuple] = []

        def fake_deepseek(system, user, model_id, api_key, thinking=None, base_url=None, urlopen=None):
            calls.append((model_id, thinking, base_url, api_key))
            return H.parse_openai_chat_turn({
                "model": "deepseek-flash",
                "choices": [{
                    "finish_reason": "stop",
                    "message": {"content": "FIXED", "reasoning_content": "hidden"},
                }],
                "usage": {
                    "completion_tokens": 15,
                    "prompt_cache_hit_tokens": 0,
                    "prompt_cache_miss_tokens": 4,
                },
            })

        def fail_anthropic(*_a, **_k):
            raise AssertionError("anthropic call_llm must not run")

        previous_deepseek = H.call_deepseek
        previous_llm = H.call_llm
        previous_ilo = H.run_ilo
        H.call_deepseek = fake_deepseek
        H.call_llm = fail_anthropic
        H.run_ilo = lambda code, ilo_bin: ("ok", "", 0)
        try:
            cell = H.run_task(
                {"id": "simple-function", "description": "add", "expected_output": "ok"},
                "ilo", "deepseek-chat", "deepseek-flash", "test-key", 2,
                "ilo", None, ".zero",
                provider="deepseek",
                thinking="disabled",
                base_url="https://api.deepseek.com",
            )
        finally:
            H.call_deepseek = previous_deepseek
            H.call_llm = previous_llm
            H.run_ilo = previous_ilo
        self.assertEqual(calls, [(
            "deepseek-flash", "disabled", "https://api.deepseek.com", "test-key",
        )])
        self.assertEqual(cell["provider"], "deepseek")
        self.assertEqual(cell["api"], "https://api.deepseek.com/chat/completions")
        self.assertEqual(cell["model"], "deepseek-chat")
        self.assertEqual(cell["model_id"], "deepseek-flash")
        self.assertEqual(cell["thinking_request"], "disabled")
        self.assertIsNone(cell["thinking_tokens"])
        self.assertEqual(cell["thinking_unknown_attempts"], 1)
        self.assertIsNone(cell["code_tokens"])
        self.assertEqual(cell["generation_tokens"], 15)
        self.assertEqual(cell["generated_chars"], len("FIXED"))
        self.assertEqual(cell["attempt_trace"][0]["code"], "FIXED")
        self.assertNotIn("hidden", cell["attempt_trace"][0]["code"])
        self.assertEqual(pr0_cell_errors(cell), [])

    def test_dry_run_deepseek_needs_no_key_and_live_refuses_without_one(self) -> None:
        env = os.environ.copy()
        for name in (
            "ANTHROPIC_API_KEY", "DEEPSEEK_API_KEY",
            "DEEPSEEK_BASE_URL", "DEEPSEEK_API_BASE",
        ):
            env.pop(name, None)
        dry = subprocess.run(
            [
                sys.executable, str(SCRIPT), "--dry-run",
                "--provider", "deepseek", "--model", "deepseek-chat",
                "--task", "simple-function",
            ],
            cwd=REPO_ROOT, env=env, capture_output=True, text=True, check=False,
        )
        self.assertEqual(dry.returncode, 0, dry.stderr)
        self.assertIn("provider: deepseek", dry.stdout)
        self.assertIn("https://api.deepseek.com/chat/completions", dry.stdout)
        self.assertIn("deepseek-chat=deepseek-flash thinking=disabled", dry.stdout)
        self.assertIn("simple-function", dry.stdout)
        self.assertNotIn("DEEPSEEK_API_KEY not set", dry.stderr)
        self.assertNotIn("ANTHROPIC_API_KEY not set", dry.stderr)

        live = subprocess.run(
            [
                sys.executable, str(SCRIPT),
                "--provider", "deepseek", "--model", "deepseek-reasoner",
                "--task", "simple-function",
            ],
            cwd=REPO_ROOT, env=env, capture_output=True, text=True, check=False,
        )
        self.assertEqual(live.returncode, 2, live.stderr)
        self.assertIn("DEEPSEEK_API_KEY not set", live.stderr)
        self.assertNotIn("Results written", live.stdout)
        self.assertNotIn("Closed-loop bench:", live.stdout)
        self.assertNotIn("chat/completions", live.stdout)


if __name__ == "__main__":
    unittest.main()
