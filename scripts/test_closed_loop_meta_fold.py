#!/usr/bin/env python3
"""Meta-stdout fold checks for closed-loop-bench. No API key and no ilo binary.

exp-05 family B: a bare number glued under `-- out:` is expected-output
text, not a statement. ilo reports ILO-P001 and the attempt fails before
the body is scored. `--fold-meta-stdout` joins that number into the
comment (M1) and ignores harness meta lines when judging stdout.

A trailing body `0` (F2b simple-function `55` then `0`) is a real
expression. This flag does not drop it.
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

P001 = '{"code":"ILO-P001","message":"expected declaration, got number `0`"}'

# F2b workflow-rollback preimage: multi-line expected `5\\n0` written as a
# comment plus a bare continuation, then the programme.
ROLLBACK = """\
-- run: main
-- out: 5
0

safe-div a:n b:n>n
  =0 b 0
  /a b
main>_
  prnt (safe-div 10 2)
  prnt (safe-div 5 0)
"""

ROLLBACK_FOLDED = """\
-- run: main
-- out: 5\\n0

safe-div a:n b:n>n
  =0 b 0
  /a b
main>_
  prnt (safe-div 10 2)
  prnt (safe-div 5 0)
"""

# F2b simple-function: the answer is printed and a trailing `0` remains.
# That `0` is the programme, not an `-- out:` continuation.
SIMPLE_TAIL = """\
-- run: main
-- out: 55

tri n:n>n
  /(*n +n 1) 2
main>_
  x=tri 10
  prnt x
  0
"""

TASK_ROLLBACK = {
    "id": "workflow-rollback",
    "description": "safe-div",
    "expected_output": "5\n0",
}

TASK_SIMPLE = {
    "id": "simple-function",
    "description": "tri",
    "expected_output": "55",
}


def load_harness():
    spec = importlib.util.spec_from_file_location(
        "closed_loop_bench_meta_fold", SCRIPT,
    )
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {SCRIPT}")
    mod = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = mod
    spec.loader.exec_module(mod)
    return mod


H = load_harness()


def fold(code: str) -> tuple[str, list[str]]:
    return H.apply_meta_stdout_fold(code)


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


class FoldRuleTests(unittest.TestCase):
    def test_f2b_workflow_rollback_preimage(self) -> None:
        got, rules = fold(ROLLBACK)
        self.assertEqual(rules, ["M1"])
        self.assertEqual(got, ROLLBACK_FOLDED)
        self.assertNotIn("\n0\n", got)
        self.assertIn("=0 b 0", got)
        self.assertIn("prnt (safe-div 5 0)", got)

    def test_second_pass_is_a_no_op(self) -> None:
        once, rules = fold(ROLLBACK)
        self.assertEqual(rules, ["M1"])
        self.assertEqual(fold(once), (once, []))

    def test_joins_a_run_of_orphan_numbers(self) -> None:
        src = "-- out: 5\n0\n1\n\nmain>_\n  prnt 1\n"
        got, rules = fold(src)
        self.assertEqual(rules, ["M1"])
        self.assertEqual(got, "-- out: 5\\n0\\n1\n\nmain>_\n  prnt 1\n")

    def test_blank_line_ends_the_continuation(self) -> None:
        src = "-- out: 5\n\n0\n\nmain>_\n  prnt 1\n"
        self.assertEqual(fold(src), (src, []))

    def test_number_that_is_the_programme_stays(self) -> None:
        src = "-- out: 0\n0\n"
        self.assertEqual(fold(src), (src, []))
        only = "-- run: main\n-- out: 0\n0\n"
        self.assertEqual(fold(only), (only, []))

    def test_a_following_comment_is_not_programme_source(self) -> None:
        src = "-- out: 0\n0\n-- run: main\n"
        self.assertEqual(fold(src), (src, []))

    def test_f2b_simple_function_trailing_zero_stays(self) -> None:
        self.assertEqual(fold(SIMPLE_TAIL), (SIMPLE_TAIL, []))

    def test_body_zero_after_a_folded_orphan_stays(self) -> None:
        src = "-- out: 5\n0\nmain>_\n  0\n"
        got, rules = fold(src)
        self.assertEqual(rules, ["M1"])
        self.assertEqual(got, "-- out: 5\\n0\nmain>_\n  0\n")

    def test_does_not_fold_a_declaration_or_a_call(self) -> None:
        src = "-- out: 5\nsafe-div a:n b:n>n\n  /a b\n"
        self.assertEqual(fold(src), (src, []))
        call = "-- out: 55\nprnt (tri 10)\n"
        self.assertEqual(fold(call), (call, []))

    def test_run_comment_is_not_an_anchor(self) -> None:
        src = "-- run: main\n0\nmain>_\n  prnt 1\n"
        self.assertEqual(fold(src), (src, []))

    def test_crlf_and_indent(self) -> None:
        src = "  -- out: 5\r\n0\r\n\r\nmain>_\r\n"
        got, rules = fold(src)
        self.assertEqual(rules, ["M1"])
        self.assertEqual(got, "  -- out: 5\\n0\r\n\r\nmain>_\r\n")

    def test_float_and_signed_numbers(self) -> None:
        src = "-- out: 1.5\n-2\n+3\nmain>_\n"
        got, rules = fold(src)
        self.assertEqual(rules, ["M1"])
        self.assertEqual(got, "-- out: 1.5\\n-2\\n+3\nmain>_\n")

    def test_empty_and_already_escaped(self) -> None:
        self.assertEqual(fold(""), ("", []))
        self.assertEqual(fold("\n"), ("\n", []))
        done = "-- out: 5\\n0\nmain>_\n  prnt 1\n"
        self.assertEqual(fold(done), (done, []))


class JudgeTests(unittest.TestCase):
    def test_meta_stdout_lines_are_ignored_when_the_value_matches(self) -> None:
        stdout = "-- out: 5\n-- run: main\n5\n0\n"
        self.assertEqual(
            H.classify_outcome("5\n0", stdout, "", 0, fold_meta_stdout=True),
            "working",
        )
        self.assertEqual(
            H.classify_outcome("5\n0", stdout, "", 0, fold_meta_stdout=False),
            "partial",
        )

    def test_trailing_value_line_stays_partial(self) -> None:
        # F2b simple-function stdout. Not meta. Do not soften to working.
        for flag in (True, False):
            self.assertEqual(
                H.classify_outcome(
                    "55", "55\n0\n", "", 0, fold_meta_stdout=flag,
                ),
                "partial",
                flag,
            )

    def test_duplicate_value_stays_partial(self) -> None:
        for flag in (True, False):
            self.assertEqual(
                H.classify_outcome(
                    "28", "28\n28\n", "", 0, fold_meta_stdout=flag,
                ),
                "partial",
                flag,
            )

    def test_nonzero_exit_stays_failed(self) -> None:
        self.assertEqual(
            H.classify_outcome(
                "5\n0", "5\n0\n", P001, 1, fold_meta_stdout=True,
            ),
            "failed",
        )

    def test_strip_keeps_a_value_that_mentions_out_mid_line(self) -> None:
        stdout = "see -- out: 5\n"
        self.assertEqual(H.strip_meta_stdout(stdout), stdout)
        self.assertEqual(
            H.strip_meta_stdout("-- out: 5\n5\n0\n-- err: no\n"),
            "5\n0\n",
        )


class LoopTests(unittest.TestCase):
    def setUp(self) -> None:
        self._call = H.call_llm
        self._ilo = H.run_ilo
        self._lang2 = H.run_lang2
        self._sleep = H.time.sleep
        H.time.sleep = lambda *_a, **_k: None

    def tearDown(self) -> None:
        H.call_llm = self._call
        H.run_ilo = self._ilo
        H.run_lang2 = self._lang2
        H.time.sleep = self._sleep

    def _run(
        self,
        flag: bool,
        first: str,
        *,
        task: dict,
        ilo,
        second: str = "main>_\n  prnt 1\n",
        header_recovery: bool = False,
    ) -> tuple[dict, list[str], list[str]]:
        prompts: list[str] = []
        seen: list[str] = []

        def fake_llm(system: str, user: str, model_id: str, api_key: str) -> dict:
            prompts.append(user)
            text = first if len(prompts) == 1 else second
            return H.parse_provider_turn({
                "model": "claude-haiku-4-5",
                "stop_reason": "end_turn",
                "content": text,
                "usage": {"input_tokens": 10, "output_tokens": 5},
            })

        def fake_ilo(code: str, ilo_bin: str) -> tuple[str, str, int]:
            seen.append(code)
            return ilo(code)

        H.call_llm = fake_llm
        H.run_ilo = fake_ilo
        cell = H.run_task(
            task,
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
            header_recovery=header_recovery,
            fold_meta_stdout=flag,
        )
        return cell, prompts, seen

    def test_flag_on_scores_the_programme_instead_of_p001(self) -> None:
        def ilo(code: str) -> tuple[str, str, int]:
            if "\n0\n" in code and "-- out: 5\\n0" not in code:
                return "", P001, 1
            return "5\n0\n", "", 0

        cell, prompts, seen = self._run(
            True, ROLLBACK, task=TASK_ROLLBACK, ilo=ilo,
        )
        self.assertEqual(seen, [ROLLBACK_FOLDED])
        self.assertEqual(len(prompts), 1)
        self.assertEqual(cell["final_outcome"], "working")
        self.assertEqual(cell["attempts_total"], 1)
        self.assertEqual(cell["attempts_to_success"], 1)
        self.assertTrue(cell["fold_meta_stdout"])
        self.assertTrue(cell["meta_recovery_applied"])
        self.assertEqual(cell["meta_recovery_rules"], ["M1"])
        self.assertEqual(cell["header_recovery_rules"], [])
        turn = cell["attempt_trace"][0]
        self.assertEqual(turn["code"], ROLLBACK_FOLDED)
        self.assertEqual(turn["stdout"], "5\n0\n")
        self.assertEqual(turn["meta_recovery_rules"], ["M1"])
        self.assertTrue(turn["meta_recovery_applied"])
        self.assertNotIn("header_recovery_rules", turn)
        self.assertEqual(turn["code_chars"], len(ROLLBACK))
        self.assertEqual(cell["generated_chars"], len(ROLLBACK))
        self.assertNotEqual(len(turn["code"]), len(ROLLBACK))

    def test_flag_off_keeps_the_orphan_and_can_repair(self) -> None:
        clean = ROLLBACK_FOLDED

        def ilo(code: str) -> tuple[str, str, int]:
            if code == clean:
                return "5\n0\n", "", 0
            return "", P001, 1

        cell, prompts, seen = self._run(
            False, ROLLBACK, task=TASK_ROLLBACK, ilo=ilo, second=clean,
        )
        self.assertEqual(seen[0], ROLLBACK)
        self.assertEqual(len(prompts), 2)
        self.assertIn(ROLLBACK.strip(), prompts[1])
        self.assertIn("ILO-P001", prompts[1])
        self.assertFalse(cell["fold_meta_stdout"])
        self.assertFalse(cell["meta_recovery_applied"])
        self.assertEqual(cell["meta_recovery_rules"], [])
        self.assertNotIn("meta_recovery_rules", cell["attempt_trace"][0])
        self.assertEqual(cell["final_outcome"], "working")
        self.assertEqual(cell["attempts_total"], 2)
        self.assertEqual(
            cell["generated_chars"], len(ROLLBACK) + len(clean),
        )

    def test_flag_on_does_not_pass_a_trailing_body_zero(self) -> None:
        def ilo(code: str) -> tuple[str, str, int]:
            self.assertEqual(code, SIMPLE_TAIL)
            self.assertIn("\n  0\n", code)
            return "55\n0\n", "", 0

        cell, _prompts, seen = self._run(
            True, SIMPLE_TAIL, task=TASK_SIMPLE, ilo=ilo, second=SIMPLE_TAIL,
        )
        self.assertEqual(seen, [SIMPLE_TAIL, SIMPLE_TAIL])
        self.assertEqual(cell["final_outcome"], "partial")
        self.assertEqual(cell["meta_recovery_rules"], [])
        self.assertFalse(cell["meta_recovery_applied"])
        self.assertEqual(cell["attempt_trace"][0]["stdout"], "55\n0\n")
        self.assertEqual(cell["generated_chars"], len(SIMPLE_TAIL) * 2)

    def test_flag_on_ignores_meta_lines_on_stdout(self) -> None:
        src = "-- run: main\n-- out: 55\nmain>_\n  prnt 55\n"

        def ilo(code: str) -> tuple[str, str, int]:
            return "-- out: 55\n55\n", "", 0

        cell, prompts, seen = self._run(True, src, task=TASK_SIMPLE, ilo=ilo)
        self.assertEqual(seen, [src])
        self.assertEqual(len(prompts), 1)
        self.assertEqual(cell["final_outcome"], "working")
        self.assertEqual(cell["meta_recovery_rules"], [])
        self.assertEqual(cell["attempt_trace"][0]["stdout"], "-- out: 55\n55\n")

    def test_flag_off_meta_stdout_stays_partial(self) -> None:
        src = "-- run: main\n-- out: 55\nmain>_\n  prnt 55\n"

        def ilo(code: str) -> tuple[str, str, int]:
            return "-- out: 55\n55\n", "", 0

        cell, prompts, _seen = self._run(
            False, src, task=TASK_SIMPLE, ilo=ilo, second=src,
        )
        self.assertEqual(cell["attempt_trace"][0]["outcome"], "partial")
        self.assertEqual(len(prompts), 2)

    def test_flag_on_does_not_rewrite_a_comparator(self) -> None:
        seen: list[str] = []
        src = "print('-- out: 5')\nprint(0)\n"

        def fake_llm(system: str, user: str, model_id: str, api_key: str) -> dict:
            return H.parse_provider_turn({
                "model": "claude-haiku-4-5",
                "stop_reason": "end_turn",
                "content": src,
                "usage": {"input_tokens": 1, "output_tokens": 1},
            })

        H.call_llm = fake_llm
        H.run_lang2 = lambda code, lang2_bin, ext: (
            seen.append(code) or ("-- out: 5\n0\n", "", 0)
        )
        cell = H.run_task(
            TASK_ROLLBACK, "python", "haiku", "claude-haiku-4-5", "test-key", 2,
            "ilo", "python3", ".py",
            fold_meta_stdout=True,
        )
        self.assertEqual(seen, [src, src])
        self.assertTrue(cell["fold_meta_stdout"])
        self.assertFalse(cell["meta_recovery_applied"])
        self.assertEqual(cell["attempt_trace"][0]["code"], src)
        self.assertNotIn("meta_recovery_rules", cell["attempt_trace"][0])
        # Comparator stdout is not an ilo judge fold.
        self.assertEqual(cell["final_outcome"], "partial")
        self.assertEqual(cell["attempt_trace"][0]["outcome"], "partial")

    def test_both_flags_keep_header_rules_and_fold_meta(self) -> None:
        raw = "-- out: 5\n0\nmain()>_;prnt (safe-div 10 2)\n"
        folded_header = "-- out: 5\\n0\nmain>_;prnt (safe-div 10 2)\n"

        def ilo(code: str) -> tuple[str, str, int]:
            if code == folded_header:
                return "5\n0\n", "", 0
            return "", P001, 1

        cell, _prompts, seen = self._run(
            True,
            raw,
            task=TASK_ROLLBACK,
            ilo=ilo,
            header_recovery=True,
        )
        self.assertEqual(seen, [folded_header])
        self.assertEqual(cell["header_recovery_rules"], ["R1"])
        self.assertEqual(cell["meta_recovery_rules"], ["M1"])
        turn = cell["attempt_trace"][0]
        self.assertEqual(turn["header_recovery_rules"], ["R1"])
        self.assertEqual(turn["meta_recovery_rules"], ["M1"])
        self.assertEqual(turn["code_pre_recovery"], raw)
        self.assertEqual(turn["code_chars"], len(raw))
        self.assertEqual(cell["final_outcome"], "working")


class DryRunFlagTest(unittest.TestCase):
    def test_dry_run_defaults_off(self) -> None:
        proc = dry_run(["--provider", "deepseek", "--model", "deepseek-chat"])
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("fold_meta_stdout: off", proc.stdout)
        self.assertNotIn("DEEPSEEK_API_KEY", proc.stdout)
        self.assertNotIn("DEEPSEEK_API_KEY", proc.stderr)

    def test_dry_run_flag_on(self) -> None:
        proc = dry_run([
            "--provider", "deepseek", "--model", "deepseek-chat",
            "--header-recovery", "--fold-meta-stdout",
        ])
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("fold_meta_stdout: on", proc.stdout)
        self.assertIn("header_recovery: on", proc.stdout)
        self.assertIn("M1", proc.stdout)
        self.assertIn("trailing body 0", proc.stdout)

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
        self.assertIn("--fold-meta-stdout", proc.stdout)
        self.assertIn("ILO-P001", proc.stdout)
        self.assertIn("Default off", proc.stdout)
        self.assertNotIn("R7", proc.stdout)


if __name__ == "__main__":
    unittest.main()
