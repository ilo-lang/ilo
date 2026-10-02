#!/usr/bin/env python3
"""Offline unit checks for the exp-11 constrain backend.

No API key. No ilo binary: check invocations are mocked.
"""

from __future__ import annotations

import json
import unittest
from typing import Any

import constrain_backend as cb


class _Proc:
    def __init__(self, returncode: int, stdout: str = "", stderr: str = "") -> None:
        self.returncode = returncode
        self.stdout = stdout
        self.stderr = stderr


class ConstrainBackendTests(unittest.TestCase):
    def test_factory_none(self) -> None:
        b = cb.make_constrain_backend("none")
        self.assertEqual(b.name, "none")
        r = b.validate_emit('main>t\n"hi"')
        self.assertTrue(r.ok)

    def test_factory_reject_empty_programme(self) -> None:
        b = cb.make_constrain_backend("reject-retry", ilo_bin="ilo")
        self.assertEqual(b.name, "reject-retry")
        r = b.validate_emit("")
        self.assertFalse(r.ok)
        self.assertIn("EMPTY", r.codes)
        self.assertIn("ILO-CONSTRAIN", r.repair_signal or "")

    def test_attach_request_noop_identity(self) -> None:
        body = {"model": "deepseek-flash", "messages": []}
        out = cb.NoopBackend().attach_request(body)
        self.assertTrue(out is body or out == body)
        out2 = cb.RejectRetryBackend().attach_request(dict(body))
        self.assertEqual(out2["model"], "deepseek-flash")
        self.assertNotIn("response_format", out2)
        self.assertNotIn("tools", out2)

    def test_local_mask_refuses(self) -> None:
        b = cb.make_constrain_backend("local-mask")
        with self.assertRaises(NotImplementedError):
            b.attach_request({})
        with self.assertRaises(NotImplementedError):
            b.validate_emit("main>t")

    def test_unknown_mode(self) -> None:
        with self.assertRaises(ValueError):
            cb.make_constrain_backend("logit-mask")

    def test_ndjson_stderr_yields_code_without_ilo(self) -> None:
        seen: list[list[str]] = []

        def fake_run(args: list[str], **_kwargs: Any) -> _Proc:
            seen.append(list(args))
            return _Proc(
                1,
                stdout="",
                stderr=(
                    '{"severity":"error","code":"ILO-P023","message":"guard"}\n'
                ),
            )

        previous = cb.subprocess.run
        cb.subprocess.run = fake_run  # type: ignore[assignment]
        try:
            result = cb.RejectRetryBackend(ilo_bin="ilo-bin").validate_emit(
                'main>t\n  ?d "down" {"healthy"}\n'
            )
        finally:
            cb.subprocess.run = previous
        self.assertFalse(result.ok)
        self.assertEqual(result.codes, ["ILO-P023"])
        self.assertIn("ILO-CONSTRAIN", result.repair_signal or "")
        self.assertIn("P023", result.repair_signal or "")
        self.assertTrue(seen)
        self.assertEqual(seen[0][0], "ilo-bin")
        self.assertEqual(seen[0][1], "check")
        self.assertIn("--json", seen[0])
        self.assertTrue(any(arg.endswith(".ilo") for arg in seen[0]))

    def test_non_target_code_comes_from_json_not_the_family_list(self) -> None:
        def fake_run(args: list[str], **_kwargs: Any) -> _Proc:
            return _Proc(
                1,
                stderr='{"code":"ILO-P003","message":"expected >"}\n',
            )

        previous = cb.subprocess.run
        cb.subprocess.run = fake_run  # type: ignore[assignment]
        try:
            result = cb.RejectRetryBackend().validate_emit("main()>t\n")
        finally:
            cb.subprocess.run = previous
        self.assertEqual(result.codes, ["ILO-P003"])

    def test_plain_stderr_target_family(self) -> None:
        codes = cb._codes_from_check(None, "error: P009 mixed conditional\n")
        self.assertEqual(codes, ["ILO-P009"])

    def test_clean_check_is_ok(self) -> None:
        def fake_run(args: list[str], **_kwargs: Any) -> _Proc:
            return _Proc(0, stdout="", stderr="")

        previous = cb.subprocess.run
        cb.subprocess.run = fake_run  # type: ignore[assignment]
        try:
            result = cb.RejectRetryBackend().validate_emit('main>t\n"hi"\n')
        finally:
            cb.subprocess.run = previous
        self.assertTrue(result.ok)
        self.assertEqual(result.codes, [])

    def test_object_stdout_json(self) -> None:
        payload = {"diagnostics": [{"code": "ILO-P007"}]}

        def fake_run(args: list[str], **_kwargs: Any) -> _Proc:
            return _Proc(1, stdout=json.dumps(payload), stderr="")

        previous = cb.subprocess.run
        cb.subprocess.run = fake_run  # type: ignore[assignment]
        try:
            result = cb.RejectRetryBackend().validate_emit("f x:L>t\n  x\n")
        finally:
            cb.subprocess.run = previous
        self.assertEqual(result.codes, ["ILO-P007"])
        self.assertIn("P007", result.repair_signal or "")


if __name__ == "__main__":
    unittest.main()
