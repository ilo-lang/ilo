#!/usr/bin/env python3
"""Repair-shape hint checks for closed-loop-bench. No API key and no ilo binary.

The gloss is the exp-03 ILO-P003 block. It is appended inside
make_repair_prompt only when --repair-shape-hint is on and the repair
error is ILO-P003 or the paren-header pair.
"""

from __future__ import annotations

import importlib.util
import os
import subprocess
import sys
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SCRIPT = REPO_ROOT / "scripts" / "closed-loop-bench.py"

P003 = '{"code":"ILO-P003","message":"expected `>`, got `(`"}'
PAREN_ONLY = "parse error: expected `>`, got `(`"
OTHER = "ILO-T005: type mismatch"
TASK = {
    "id": "simple-function",
    "description": "print tri",
    "expected_output": "1",
}


def load_harness():
    spec = importlib.util.spec_from_file_location(
        "closed_loop_bench_repair_hint", SCRIPT,
    )
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {SCRIPT}")
    mod = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = mod
    spec.loader.exec_module(mod)
    return mod


H = load_harness()


def repair(error: str, flag: bool, lang: str = "ilo", code: str = "main()>_;prnt 1") -> str:
    return H.make_repair_prompt(
        TASK, "docs", error, lang, code, repair_shape_hint=flag,
    )


def dry_run(extra: list[str]) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env.pop("ANTHROPIC_API_KEY", None)
    env.pop("DEEPSEEK_API_KEY", None)
    return subprocess.run(
        [sys.executable, str(SCRIPT), "--dry-run", *extra],
        cwd=REPO_ROOT,
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )


class GlossTextTest(unittest.TestCase):
    def test_fixed_gloss_names_header_and_one_liner(self) -> None:
        text = H.REPAIR_SHAPE_HINT
        self.assertTrue(text.startswith("SHAPE:"))
        self.assertIn("`name params>ret;body`", text)
        self.assertIn("`main>_;…`", text)
        self.assertIn("`main()>_;…`", text)
        self.assertIn("`tri n:n>n;+n 1`", text)
        self.assertIn("`main>_;prnt (tri 10)`", text)
        self.assertIn("Call sites may use", text)


class MakeRepairPromptTest(unittest.TestCase):
    def test_flag_on_and_p003_contains_gloss(self) -> None:
        got = repair(P003, True)
        self.assertIn(H.REPAIR_SHAPE_HINT, got)
        self.assertIn(P003, got)
        self.assertIn("Previous program:", got)
        self.assertLess(got.index(P003), got.index(H.REPAIR_SHAPE_HINT))
        self.assertLess(
            got.index(H.REPAIR_SHAPE_HINT),
            got.index("Rewrite the program"),
        )

    def test_flag_off_omits_gloss_when_p003_present(self) -> None:
        got = repair(P003, False)
        self.assertNotIn(H.REPAIR_SHAPE_HINT, got)
        self.assertNotIn("SHAPE:", got)
        self.assertIn(P003, got)

    def test_default_argument_is_off(self) -> None:
        got = H.make_repair_prompt(TASK, "docs", P003, "ilo", "main()>n;1")
        self.assertNotIn("SHAPE:", got)

    def test_flag_on_other_error_omits_gloss(self) -> None:
        got = repair(OTHER, True)
        self.assertNotIn("SHAPE:", got)
        self.assertIn(OTHER, got)

    def test_similar_paren_header_without_code(self) -> None:
        self.assertTrue(H.repair_shape_hint_applies(PAREN_ONLY))
        self.assertIn(H.REPAIR_SHAPE_HINT, repair(PAREN_ONLY, True))
        self.assertNotIn("SHAPE:", repair(PAREN_ONLY, False))

    def test_any_ilo_p003_applies(self) -> None:
        err = "ILO-P003: expected `{`, got Ident"
        self.assertTrue(H.repair_shape_hint_applies(err))
        self.assertIn(H.REPAIR_SHAPE_HINT, repair(err, True))

    def test_unrelated_expected_gt_does_not_apply(self) -> None:
        err = "ILO-P011: expected `>` in a binding"
        self.assertFalse(H.repair_shape_hint_applies(err))
        self.assertNotIn("SHAPE:", repair(err, True))

    def test_comparator_arm_skips_ilo_gloss(self) -> None:
        got = repair(P003, True, lang="python")
        self.assertNotIn("SHAPE:", got)


class RunTaskWiringTest(unittest.TestCase):
    def setUp(self) -> None:
        self._call = H.call_llm
        self._ilo = H.run_ilo
        self._sleep = H.time.sleep
        H.time.sleep = lambda *_a, **_k: None

    def tearDown(self) -> None:
        H.call_llm = self._call
        H.run_ilo = self._ilo
        H.time.sleep = self._sleep

    def _run(self, flag: bool) -> tuple[dict, list[str]]:
        prompts: list[str] = []

        def fake_llm(system: str, user: str, model_id: str, api_key: str) -> dict:
            prompts.append(user)
            if len(prompts) == 1:
                return H.parse_provider_turn({
                    "model": "claude-haiku-4-5",
                    "stop_reason": "end_turn",
                    "content": "main()>_;prnt 1",
                    "usage": {"input_tokens": 10, "output_tokens": 5},
                })
            return H.parse_provider_turn({
                "model": "claude-haiku-4-5",
                "stop_reason": "end_turn",
                "content": "main>_;prnt 1",
                "usage": {
                    "input_tokens": 12,
                    "output_tokens": 4,
                    "output_tokens_details": {"thinking_tokens": 0},
                },
            })

        def fake_ilo(code: str, ilo_bin: str) -> tuple[str, str, int]:
            if "main()>" in code:
                return "", P003, 1
            return "1\n", "", 0

        H.call_llm = fake_llm
        H.run_ilo = fake_ilo
        cell = H.run_task(
            TASK,
            "ilo",
            "haiku",
            "claude-haiku-4-5",
            "test-key",
            2,
            "ilo",
            None,
            ".zero",
            ilo_modules=["ilo-language"],
            ilo_context_text="docs",
            repair_shape_hint=flag,
        )
        return cell, prompts

    def test_flag_on_repair_prompt_contains_gloss(self) -> None:
        cell, prompts = self._run(True)
        self.assertEqual(len(prompts), 2)
        self.assertNotIn(H.REPAIR_SHAPE_HINT, prompts[0])
        self.assertIn(H.REPAIR_SHAPE_HINT, prompts[1])
        self.assertIn("ILO-P003", prompts[1])
        self.assertTrue(cell["repair_shape_hint"])
        self.assertEqual(cell["final_outcome"], "working")

    def test_flag_off_repair_prompt_omits_gloss(self) -> None:
        cell, prompts = self._run(False)
        self.assertEqual(len(prompts), 2)
        self.assertNotIn("SHAPE:", prompts[1])
        self.assertIn("ILO-P003", prompts[1])
        self.assertFalse(cell["repair_shape_hint"])


class DryRunFlagTest(unittest.TestCase):
    def test_dry_run_defaults_off(self) -> None:
        proc = dry_run(["--provider", "deepseek", "--model", "deepseek-chat"])
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("repair_shape_hint: off", proc.stdout)
        self.assertNotIn("DEEPSEEK_API_KEY", proc.stderr)

    def test_dry_run_flag_on(self) -> None:
        proc = dry_run([
            "--provider", "deepseek", "--model", "deepseek-chat",
            "--repair-shape-hint",
        ])
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("repair_shape_hint: on", proc.stdout)
        self.assertIn("ILO-P003", proc.stdout)

    def test_help_documents_flag(self) -> None:
        env = os.environ.copy()
        env.pop("ANTHROPIC_API_KEY", None)
        env.pop("DEEPSEEK_API_KEY", None)
        proc = subprocess.run(
            [sys.executable, str(SCRIPT), "--help"],
            cwd=REPO_ROOT,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("--repair-shape-hint", proc.stdout)
        self.assertIn("ILO-P003", proc.stdout)
        self.assertIn("main>_;", proc.stdout)


if __name__ == "__main__":
    unittest.main()
