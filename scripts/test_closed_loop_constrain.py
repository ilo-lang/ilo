#!/usr/bin/env python3
"""exp-11 constrain hook on the closed-loop harness. No API key.

A fixture illegal emit is rejected when --constrain reject-retry is on.
ilo check is a mocked subprocess. The programme is not run. The repair
signal is the next user message and the reject counts toward retry-cap.
Default mode is none: soft-edge and header recovery stay off.
"""

from __future__ import annotations

import importlib.util
import os
import subprocess
import sys
import unittest
from pathlib import Path
from typing import Any

import constrain_backend as cb

REPO_ROOT = Path(__file__).resolve().parent.parent
SCRIPT = REPO_ROOT / "scripts" / "closed-loop-bench.py"

# Prefix-ternary mixed with a brace else. exp-09 would rewrite this under
# soft edges; reject-retry must see it raw and must not run it.
ILLEGAL = 'main>t\n  d=true\n  ?d "down" {"healthy"}\n'
GOOD = 'main>t\n"ok"\n'


def load_harness():
    spec = importlib.util.spec_from_file_location(
        "closed_loop_bench_constrain", SCRIPT,
    )
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {SCRIPT}")
    mod = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = mod
    spec.loader.exec_module(mod)
    return mod


H = load_harness()


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


def _chat_turn(content: str) -> dict[str, Any]:
    return H.parse_openai_chat_turn({
        "model": "deepseek-flash",
        "choices": [{
            "finish_reason": "stop",
            "message": {"role": "assistant", "content": content},
        }],
        "usage": {
            "completion_tokens": 11,
            "prompt_cache_hit_tokens": 0,
            "prompt_cache_miss_tokens": 4,
        },
    })


class _Proc:
    def __init__(self, returncode: int, stdout: str = "", stderr: str = "") -> None:
        self.returncode = returncode
        self.stdout = stdout
        self.stderr = stderr


class ConstrainHarnessTests(unittest.TestCase):
    def test_dry_run_none_and_reject_retry_leave_rails_off(self) -> None:
        off = dry_run(["--constrain", "none"])
        self.assertEqual(off.returncode, 0, off.stderr)
        self.assertIn("constrain: none", off.stdout)
        self.assertIn("soft_edge_recovery: off", off.stdout)
        self.assertIn("header_recovery: off", off.stdout)
        self.assertIn("repair_shape_hint: off", off.stdout)

        on = dry_run(["--constrain", "reject-retry", "--provider", "deepseek"])
        self.assertEqual(on.returncode, 0, on.stderr)
        self.assertIn("constrain: reject-retry", on.stdout)
        self.assertIn("soft_edge_recovery: off", on.stdout)
        self.assertIn("header_recovery: off", on.stdout)
        self.assertIn("cannot attach logit or grammar masks", on.stdout)
        self.assertIn("feasible substitute", on.stdout)

    def test_local_mask_dry_run_and_live_refusal_need_no_key(self) -> None:
        dry = dry_run(["--constrain", "local-mask"])
        self.assertEqual(dry.returncode, 0, dry.stderr)
        self.assertIn("constrain: local-mask", dry.stdout)
        self.assertIn("not implemented", dry.stdout)

        env = os.environ.copy()
        env.pop("ANTHROPIC_API_KEY", None)
        env.pop("DEEPSEEK_API_KEY", None)
        live = subprocess.run(
            [
                sys.executable, str(SCRIPT),
                "--constrain", "local-mask",
                "--provider", "deepseek",
                "--model", "deepseek-chat",
                "--task", "simple-function",
            ],
            cwd=REPO_ROOT,
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(live.returncode, 2, live.stdout)
        self.assertIn("local-mask is not implemented", live.stderr)
        self.assertIn("cannot attach logit or grammar masks", live.stderr)
        self.assertNotIn("api.deepseek.com", live.stderr + live.stdout)

    def test_illegal_emit_rejects_before_run_and_feeds_repair(self) -> None:
        """Fixture illegal emit: reject, do not run, count both turns."""
        users: list[str] = []
        ran: list[str] = []
        emits = [ILLEGAL, GOOD]

        def fake_deepseek(
            system: str,
            user: str,
            model_id: str,
            api_key: str,
            thinking: str | None = None,
            base_url: str | None = None,
            urlopen: Any = None,
            constrain: Any = None,
        ) -> dict[str, Any]:
            users.append(user)
            self.assertIsNotNone(constrain)
            body = {"model": model_id, "messages": []}
            attached = constrain.attach_request(dict(body))
            self.assertEqual(set(attached), set(body))
            content = emits[min(len(users) - 1, len(emits) - 1)]
            return _chat_turn(content)

        def fake_run(code: str, ilo_bin: str) -> tuple[str, str, int]:
            ran.append(code)
            return ("ok\n", "", 0)

        def fake_check(args: list[str], **_kwargs: Any) -> _Proc:
            path = next(arg for arg in args if str(arg).endswith(".ilo"))
            text = Path(path).read_text(encoding="utf-8")
            if "?d" in text:
                return _Proc(
                    1,
                    stderr=(
                        '{"severity":"error","code":"ILO-P023",'
                        '"message":"braceless guard"}\n'
                    ),
                )
            return _Proc(0, stdout="", stderr="")

        previous_deepseek = H.call_deepseek
        previous_ilo = H.run_ilo
        previous_check = cb.subprocess.run
        H.call_deepseek = fake_deepseek
        H.run_ilo = fake_run
        cb.subprocess.run = fake_check  # type: ignore[assignment]
        try:
            cell = H.run_task(
                {
                    "id": "csv-sales-summary",
                    "description": "summarise sales",
                    "expected_output": "ok",
                },
                "ilo", "deepseek-chat", "deepseek-flash", "test-key", 2,
                "ilo", None, ".zero",
                provider="deepseek",
                thinking="disabled",
                base_url="https://api.deepseek.com",
                constrain="reject-retry",
                # Rails stay independently off. Passing them on here only
                # proves the reject happens before either rewrite.
                header_recovery=True,
                soft_edge_recovery=True,
            )
        finally:
            H.call_deepseek = previous_deepseek
            H.run_ilo = previous_ilo
            cb.subprocess.run = previous_check

        self.assertEqual(cell["constrain_mode"], "reject-retry")
        self.assertEqual(cell["constrain_reject_count"], 1)
        self.assertEqual(cell["constrain_reject_codes"], ["ILO-P023"])
        self.assertEqual(cell["final_outcome"], "working")
        self.assertEqual(cell["attempts_to_success"], 2)
        self.assertEqual(cell["attempts_total"], 2)
        self.assertEqual(cell["success_rate"], 1.0)
        self.assertFalse(cell["header_recovery_applied"])
        self.assertEqual(cell["header_recovery_rules"], [])
        self.assertFalse(cell["soft_edge_recovery_applied"])
        self.assertEqual(cell["soft_edge_recovery_rules"], [])
        # Only the accepted programme ran. The illegal emit did not.
        self.assertEqual(ran, [GOOD])
        self.assertEqual(len(users), 2)
        self.assertIn("ILO-CONSTRAIN", users[1])
        self.assertIn("ILO-P023", users[1])
        self.assertIn(ILLEGAL.strip().splitlines()[0], users[1])

        rejected, accepted = cell["attempt_trace"]
        self.assertTrue(rejected["constrain_rejected"])
        self.assertEqual(rejected["constrain_reject_codes"], ["ILO-P023"])
        self.assertEqual(rejected["constrain_mode"], "reject-retry")
        self.assertEqual(rejected["outcome"], "failed")
        self.assertIn('?d "down"', rejected["code"])
        self.assertEqual(rejected["header_recovery_rules"], [])
        self.assertEqual(rejected["soft_edge_recovery_rules"], [])
        self.assertNotEqual(rejected["stdout"], "ok\n")
        self.assertFalse(accepted["constrain_rejected"])
        self.assertEqual(accepted["code"], GOOD)
        self.assertEqual(accepted["outcome"], "working")
        # Both turns are billed, so the reject is inside the retry cap.
        self.assertTrue(rejected["billed"])
        self.assertTrue(accepted["billed"])
        self.assertEqual(cell["generated_chars"], len(ILLEGAL) + len(GOOD))

    def test_none_does_not_check_and_keeps_defaults(self) -> None:
        def boom_check(*_args: Any, **_kwargs: Any) -> None:
            raise AssertionError("ilo check must not run when constrain is none")

        ran: list[str] = []

        def fake_deepseek(
            system: str,
            user: str,
            model_id: str,
            api_key: str,
            thinking: str | None = None,
            base_url: str | None = None,
            urlopen: Any = None,
        ) -> dict[str, Any]:
            return _chat_turn(ILLEGAL)

        def fake_run(code: str, ilo_bin: str) -> tuple[str, str, int]:
            ran.append(code)
            return ("ok\n", "", 0)

        previous_deepseek = H.call_deepseek
        previous_ilo = H.run_ilo
        previous_check = cb.subprocess.run
        H.call_deepseek = fake_deepseek
        H.run_ilo = fake_run
        cb.subprocess.run = boom_check  # type: ignore[assignment]
        try:
            cell = H.run_task(
                {
                    "id": "csv-sales-summary",
                    "description": "summarise sales",
                    "expected_output": "ok",
                },
                "ilo", "deepseek-chat", "deepseek-flash", "test-key", 1,
                "ilo", None, ".zero",
                provider="deepseek",
                thinking="disabled",
                constrain="none",
            )
        finally:
            H.call_deepseek = previous_deepseek
            H.run_ilo = previous_ilo
            cb.subprocess.run = previous_check

        self.assertEqual(ran, [ILLEGAL])
        self.assertEqual(cell["constrain_mode"], "none")
        self.assertEqual(cell["constrain_reject_count"], 0)
        self.assertEqual(cell["constrain_reject_codes"], [])
        self.assertFalse(cell["attempt_trace"][0]["constrain_rejected"])
        self.assertFalse(cell["soft_edge_recovery"])
        self.assertFalse(cell["header_recovery"])
        self.assertEqual(cell["final_outcome"], "working")

    def test_reject_retry_attach_does_not_add_a_mask_field(self) -> None:
        captured: dict[str, Any] = {}

        class _Resp:
            def read(self) -> bytes:
                return (
                    b'{"model":"deepseek-flash","choices":[{"finish_reason":"stop",'
                    b'"message":{"content":"main>t"}}],"usage":{"completion_tokens":3,'
                    b'"prompt_cache_miss_tokens":1,"prompt_cache_hit_tokens":0}}'
                )

            def __enter__(self) -> "_Resp":
                return self

            def __exit__(self, *_exc: object) -> bool:
                return False

        def fake_urlopen(req: Any, timeout: int = 0) -> _Resp:
            import json
            captured["body"] = json.loads(req.data.decode())
            return _Resp()

        backend = cb.make_constrain_backend("reject-retry", ilo_bin="ilo")
        H.call_deepseek(
            "system", "user", "deepseek-flash", "secret",
            thinking="disabled",
            base_url="https://api.deepseek.com",
            urlopen=fake_urlopen,
            constrain=backend,
        )
        body = captured["body"]
        self.assertEqual(body["model"], "deepseek-flash")
        self.assertNotIn("response_format", body)
        self.assertNotIn("tools", body)
        self.assertNotIn("logit_bias", body)
        self.assertNotIn("guided_grammar", body)

    def test_local_mask_run_task_refuses_before_the_model(self) -> None:
        def boom(*_a: Any, **_k: Any) -> None:
            raise AssertionError("model must not be called")

        previous_deepseek = H.call_deepseek
        previous_llm = H.call_llm
        H.call_deepseek = boom
        H.call_llm = boom
        try:
            with self.assertRaises(ValueError) as ctx:
                H.run_task(
                    {
                        "id": "csv-sales-summary",
                        "description": "summarise",
                        "expected_output": "ok",
                    },
                    "ilo", "deepseek-chat", "deepseek-flash", "test-key", 1,
                    "ilo", None, ".zero",
                    provider="deepseek",
                    constrain="local-mask",
                )
        finally:
            H.call_deepseek = previous_deepseek
            H.call_llm = previous_llm
        self.assertIn("not implemented", str(ctx.exception))
        self.assertIn("grammar", str(ctx.exception))

    def test_both_rejects_stay_inside_retry_cap(self) -> None:
        calls = {"n": 0}

        def fake_deepseek(*_a: Any, **_k: Any) -> dict[str, Any]:
            calls["n"] += 1
            return _chat_turn(ILLEGAL)

        def fake_run(code: str, ilo_bin: str) -> tuple[str, str, int]:
            raise AssertionError("rejected programme must not run")

        def fake_check(args: list[str], **_kwargs: Any) -> _Proc:
            return _Proc(1, stderr="ILO-P009 mixed conditional\n")

        previous_deepseek = H.call_deepseek
        previous_ilo = H.run_ilo
        previous_check = cb.subprocess.run
        H.call_deepseek = fake_deepseek
        H.run_ilo = fake_run
        cb.subprocess.run = fake_check  # type: ignore[assignment]
        try:
            cell = H.run_task(
                {
                    "id": "record-normalize",
                    "description": "normalise",
                    "expected_output": "ok",
                },
                "ilo", "deepseek-chat", "deepseek-flash", "test-key", 2,
                "ilo", None, ".zero",
                provider="deepseek",
                constrain="reject-retry",
            )
        finally:
            H.call_deepseek = previous_deepseek
            H.run_ilo = previous_ilo
            cb.subprocess.run = previous_check

        self.assertEqual(calls["n"], 2)
        self.assertEqual(cell["constrain_reject_count"], 2)
        self.assertEqual(cell["attempts_total"], 2)
        self.assertEqual(cell["constrain_reject_codes"], ["ILO-P009"])
        self.assertIsNone(cell["attempts_to_success"])
        self.assertEqual(cell["final_outcome"], "failed")
        self.assertEqual(cell["success_rate"], 0.0)


if __name__ == "__main__":
    unittest.main()
