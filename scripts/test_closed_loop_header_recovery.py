#!/usr/bin/env python3
"""Header-recovery checks for closed-loop-bench. No API key and no ilo binary.

exp-04 rules R4, R3, R1, R2, R5 run once on extracted ilo text, before the
ilo invoke, only when --header-recovery is on. Call-site (…) is not rewritten.
"""

from __future__ import annotations

import hashlib
import importlib.util
import os
import subprocess
import sys
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SCRIPT = REPO_ROOT / "scripts" / "closed-loop-bench.py"

TASK = {
    "id": "simple-function",
    "description": "print tri",
    "expected_output": "1",
}
P003 = '{"code":"ILO-P003","message":"expected `>`, got `(`"}'


def load_harness():
    spec = importlib.util.spec_from_file_location(
        "closed_loop_bench_header_recovery", SCRIPT,
    )
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {SCRIPT}")
    mod = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = mod
    spec.loader.exec_module(mod)
    return mod


H = load_harness()


def recover(code: str) -> tuple[str, list[str]]:
    return H.apply_header_recovery(code)


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


class RuleTests(unittest.TestCase):
    def test_r1_empty_paren_before_gt(self) -> None:
        self.assertEqual(recover("main()>_;"), ("main>_;", ["R1"]))
        self.assertEqual(recover("main()>n"), ("main>n", ["R1"]))
        self.assertEqual(recover("main()>t;"), ("main>t;", ["R1"]))
        self.assertEqual(
            recover("main() >_;"),
            ("main>_;", ["R1"]),
        )

    def test_r1_same_line_body_keeps_call_site(self) -> None:
        src = "main()>_;prnt (quad 7)"
        self.assertEqual(recover(src), ("main>_;prnt (quad 7)", ["R1"]))

    def test_r2_bare_empty_paren_header_line(self) -> None:
        self.assertEqual(recover("main()"), ("main>_;", ["R2"]))
        self.assertEqual(recover("main();"), ("main>_;", ["R2"]))
        self.assertEqual(recover("  main()  "), ("  main>_;", ["R2"]))
        src = "main()\n  prnt (tri 10)\n"
        got, rules = recover(src)
        self.assertEqual(rules, ["R2"])
        self.assertEqual(got, "main>_;\n  prnt (tri 10)\n")

    def test_r3_fake_c_signature(self) -> None:
        self.assertEqual(recover("main():_>_"), ("main>_", ["R3"]))
        self.assertEqual(recover("main():n>n"), ("main>n", ["R3"]))

    def test_r4_strips_f_and_fn_on_header_lines(self) -> None:
        self.assertEqual(
            recover("f double x:n>n;*x 2"),
            ("double x:n>n;*x 2", ["R4"]),
        )
        self.assertEqual(
            recover("fn double x:n>n;*x 2"),
            ("double x:n>n;*x 2", ["R4"]),
        )
        self.assertEqual(
            recover("f main()>_;prnt (quad 7)"),
            ("main>_;prnt (quad 7)", ["R4", "R1"]),
        )
        self.assertEqual(
            recover("fn main()>_;"),
            ("main>_;", ["R4", "R1"]),
        )
        self.assertEqual(recover("f main()"), ("main>_;", ["R4", "R2"]))
        self.assertEqual(recover("fn main():_>_"), ("main>_", ["R4", "R3"]))

    def test_r4_leaves_non_header_keyword_lines(self) -> None:
        for src in ("f main", "fn main", "function main()>_;", "for x:n>n"):
            self.assertEqual(recover(src), (src, []), src)

    def test_r5_bare_main_header(self) -> None:
        self.assertEqual(recover("main"), ("main>_", ["R5"]))
        self.assertEqual(recover("  main"), ("  main>_", ["R5"]))
        src = "main\n  prnt 1\n"
        self.assertEqual(recover(src), ("main>_\n  prnt 1\n", ["R5"]))
        self.assertEqual(recover("main \n"), ("main>_\n", ["R5"]))

    def test_rule_ids_follow_application_order_once(self) -> None:
        src = "main()>n\nf double x:n>n;1\nmain()>t;\n"
        got, rules = recover(src)
        self.assertEqual(rules, ["R4", "R1"])
        self.assertEqual(got, "main>n\ndouble x:n>n;1\nmain>t;\n")

    def test_hyphenated_name_and_newline_endings(self) -> None:
        self.assertEqual(recover("foo-bar()>n\n"), ("foo-bar>n\n", ["R1"]))
        self.assertEqual(recover("main()\r\n"), ("main>_;\r\n", ["R2"]))
        self.assertEqual(recover(""), ("", []))
        self.assertEqual(recover("\n"), ("\n", []))

    def test_second_pass_is_a_no_op(self) -> None:
        once, rules = recover("f main()>_;prnt (quad 7)")
        self.assertEqual(rules, ["R4", "R1"])
        self.assertEqual(recover(once), (once, []))


class CallSiteTests(unittest.TestCase):
    def test_call_sites_and_good_headers_are_unchanged(self) -> None:
        samples = [
            "prnt (tri 10)",
            "(double (double x))",
            "main>_;prnt (tri 10)",
            "tri n:n>n;+n 1\nmain>_;prnt (tri 10)\n",
            "main>_;prnt (foo())",
            "main>_;foo()",
            "main>_;foo()>",
            "double(n)>n",
            "foo(x)>n",
            "main( )>",
            "main;",
        ]
        for src in samples:
            self.assertEqual(recover(src), (src, []), src)


class P003ShapeTests(unittest.TestCase):
    def test_p003_family_is_detected_on_the_raw_emit(self) -> None:
        self.assertTrue(H.p003_header_shape("main()>_;"))
        self.assertTrue(H.p003_header_shape("main()"))
        self.assertTrue(H.p003_header_shape("main():_>_"))
        self.assertTrue(H.p003_header_shape("main"))
        self.assertTrue(H.p003_header_shape("f main()>_;"))
        self.assertTrue(H.p003_header_shape("fn main()"))

    def test_r4_only_and_call_sites_are_not_p003(self) -> None:
        self.assertFalse(H.p003_header_shape("f double x:n>n;*x 2"))
        self.assertFalse(H.p003_header_shape("main>_;prnt (tri 10)"))
        self.assertFalse(H.p003_header_shape("prnt (tri 10)"))
        self.assertFalse(H.p003_header_shape("(double (double x))"))
        self.assertFalse(H.p003_header_shape("main>_;prnt (foo())"))
        self.assertFalse(H.p003_header_shape("fn main"))


class RunTaskWiringTest(unittest.TestCase):
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
        ilo_ok,
        second: str = "main>_;prnt 1",
        repair_shape_hint: bool = False,
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
            if ilo_ok(code):
                return "1\n", "", 0
            return "", P003, 1

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
            repair_shape_hint=repair_shape_hint,
            header_recovery=flag,
        )
        return cell, prompts, seen

    def test_flag_on_greens_the_first_ilo_invoke(self) -> None:
        raw = "main()>_;prnt 1"
        cell, prompts, seen = self._run(
            True, raw, ilo_ok=lambda code: "main()>" not in code and "main>_;" in code,
        )
        self.assertEqual(seen, ["main>_;prnt 1"])
        self.assertEqual(len(prompts), 1)
        self.assertEqual(cell["final_outcome"], "working")
        self.assertEqual(cell["attempts_total"], 1)
        self.assertEqual(cell["attempts_to_success"], 1)
        self.assertTrue(cell["header_recovery"])
        self.assertTrue(cell["header_recovery_applied"])
        self.assertEqual(cell["header_recovery_rules"], ["R1"])
        turn = cell["attempt_trace"][0]
        self.assertEqual(turn["code"], "main>_;prnt 1")
        self.assertEqual(turn["code_raw"], raw)
        self.assertEqual(turn["code_pre_recovery"], raw)
        self.assertEqual(turn["header_recovery_rules"], ["R1"])
        self.assertTrue(turn["header_recovery_applied"])
        self.assertTrue(turn["p003_pre_recovery"])
        self.assertEqual(
            turn["header_recovery_preimage_sha256"],
            hashlib.sha256(raw.encode("utf-8")).hexdigest(),
        )
        self.assertEqual(turn["code_chars"], len(raw))
        self.assertEqual(cell["generated_chars"], len(raw))
        self.assertEqual(cell["code_chars_by_turn"], [len(raw)])
        self.assertEqual(cell["final_code_chars"], len(raw))
        self.assertNotEqual(len(turn["code"]), len(raw))

    def test_flag_off_preserves_the_emitted_programme(self) -> None:
        raw = "main()>_;prnt 1"
        cell, prompts, seen = self._run(
            False, raw, ilo_ok=lambda code: code == "main>_;prnt 1",
        )
        self.assertEqual(seen, [raw, "main>_;prnt 1"])
        self.assertEqual(len(prompts), 2)
        self.assertIn(raw, prompts[1])
        self.assertFalse(cell["header_recovery"])
        self.assertFalse(cell["header_recovery_applied"])
        self.assertEqual(cell["header_recovery_rules"], [])
        self.assertEqual(cell["attempt_trace"][0]["code"], raw)
        self.assertNotIn("code_pre_recovery", cell["attempt_trace"][0])
        self.assertNotIn("code_raw", cell["attempt_trace"][0])
        self.assertNotIn("p003_pre_recovery", cell["attempt_trace"][0])
        self.assertEqual(cell["generated_chars"], len(raw) + len("main>_;prnt 1"))
        self.assertEqual(cell["attempts_total"], 2)

    def test_omitted_flag_matches_the_off_baseline(self) -> None:
        seen: list[str] = []

        def fake_llm(system: str, user: str, model_id: str, api_key: str) -> dict:
            return H.parse_provider_turn({
                "model": "claude-haiku-4-5",
                "stop_reason": "end_turn",
                "content": "main()>_;prnt 1",
                "usage": {"input_tokens": 1, "output_tokens": 1},
            })

        H.call_llm = fake_llm
        H.run_ilo = lambda code, ilo_bin: seen.append(code) or ("1\n", "", 0)
        cell = H.run_task(
            TASK, "ilo", "haiku", "claude-haiku-4-5", "test-key", 2,
            "ilo", None, ".zero",
            ilo_modules=["ilo-language"], ilo_context_text="docs",
        )
        self.assertEqual(seen, ["main()>_;prnt 1"])
        self.assertFalse(cell["header_recovery"])
        self.assertNotIn("code_raw", cell["attempt_trace"][0])

    def test_flag_on_still_repairs_when_ilo_fails(self) -> None:
        raw = "main()>_;prnt 0"
        cell, prompts, seen = self._run(
            True,
            raw,
            ilo_ok=lambda code: code == "main>_;prnt 1",
            repair_shape_hint=True,
        )
        self.assertEqual(seen[0], "main>_;prnt 0")
        self.assertEqual(len(prompts), 2)
        self.assertIn("main>_;prnt 0", prompts[1])
        self.assertNotIn("main()>_;prnt 0", prompts[1])
        self.assertIn(H.REPAIR_SHAPE_HINT, prompts[1])
        self.assertEqual(cell["final_outcome"], "working")
        self.assertEqual(cell["attempts_total"], 2)
        self.assertTrue(cell["repair_shape_hint"])
        self.assertEqual(cell["attempt_trace"][0]["header_recovery_rules"], ["R1"])
        self.assertEqual(cell["attempt_trace"][1]["header_recovery_rules"], [])
        self.assertIsNone(cell["attempt_trace"][1]["header_recovery_preimage_sha256"])
        self.assertFalse(cell["attempt_trace"][1]["p003_pre_recovery"])

    def test_flag_on_does_not_rewrite_a_comparator(self) -> None:
        seen: list[str] = []
        src = "def main():\n    return 1\n"

        def fake_llm(system: str, user: str, model_id: str, api_key: str) -> dict:
            return H.parse_provider_turn({
                "model": "claude-haiku-4-5",
                "stop_reason": "end_turn",
                "content": src,
                "usage": {"input_tokens": 1, "output_tokens": 1},
            })

        H.call_llm = fake_llm
        H.run_lang2 = lambda code, lang2_bin, ext: seen.append(code) or ("1\n", "", 0)
        cell = H.run_task(
            TASK, "python", "haiku", "claude-haiku-4-5", "test-key", 2,
            "ilo", "python3", ".py",
            header_recovery=True,
        )
        self.assertEqual(seen, [src])
        self.assertTrue(cell["header_recovery"])
        self.assertFalse(cell["header_recovery_applied"])
        self.assertEqual(cell["attempt_trace"][0]["code"], src)
        self.assertNotIn("code_pre_recovery", cell["attempt_trace"][0])


class DryRunFlagTest(unittest.TestCase):
    def test_dry_run_defaults_off(self) -> None:
        proc = dry_run(["--provider", "deepseek", "--model", "deepseek-chat"])
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("header_recovery: off", proc.stdout)
        self.assertIn("repair_shape_hint: off", proc.stdout)
        self.assertNotIn("DEEPSEEK_API_KEY", proc.stderr)

    def test_dry_run_flag_on(self) -> None:
        proc = dry_run([
            "--provider", "deepseek", "--model", "deepseek-chat",
            "--repair-shape-hint", "--header-recovery",
        ])
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("header_recovery: on", proc.stdout)
        self.assertIn("repair_shape_hint: on", proc.stdout)
        self.assertIn("R4, R3, R1, R2, R5", proc.stdout)

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
        self.assertIn("--header-recovery", proc.stdout)
        self.assertIn("name>_;", proc.stdout)
        self.assertIn("main>_", proc.stdout)
        self.assertIn("Default off", proc.stdout)


if __name__ == "__main__":
    unittest.main()
