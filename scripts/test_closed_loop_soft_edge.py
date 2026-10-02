#!/usr/bin/env python3
"""Soft-edge recovery checks for closed-loop-bench. No API key required.

exp-09 SE3 soft L> and SE1 ternary-mix collapse run once on extracted ilo
text when --soft-edge-recovery is on, after header/meta recovery.
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


def load_harness():
    spec = importlib.util.spec_from_file_location(
        "closed_loop_bench_soft_edge", SCRIPT,
    )
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {SCRIPT}")
    mod = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = mod
    spec.loader.exec_module(mod)
    return mod


H = load_harness()


def soft(code: str) -> tuple[str, list[str]]:
    return H.apply_soft_edge_recovery(code)


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


class SoftEdgeTests(unittest.TestCase):
    def test_se3_l_gt_before_return(self) -> None:
        out, rules = soft("classify p:L>t\n  \"ok\"\n")
        self.assertEqual(out, "classify p:L _>t\n  \"ok\"\n")
        self.assertEqual(rules, ["SE3"])

    def test_se3_both_sides(self) -> None:
        out, rules = soft("norm x:L>L;\n  x\n")
        self.assertEqual(out, "norm x:L _>L _;\n  x\n")
        self.assertEqual(rules, ["SE3"])

    def test_se3_idempotent_on_explicit(self) -> None:
        src = "f x:L _>L _\n  x\n"
        out, rules = soft(src)
        self.assertEqual(out, src)
        self.assertEqual(rules, [])

    def test_se1_expression_else(self) -> None:
        src = 'main>t\n  d=true\n  ?d "down" {"healthy"}\n'
        out, rules = soft(src)
        self.assertIn('?d{"down"}{"healthy"}', out)
        self.assertEqual(rules, ["SE1"])

    def test_se1_skips_stmt_else(self) -> None:
        src = 'main>t\n  ?d "down" {deg=>=c 400;?deg "degraded" "healthy"}\n'
        out, rules = soft(src)
        self.assertEqual(out, src)
        self.assertEqual(rules, [])

    def test_se3_and_se1_together(self) -> None:
        src = 'f x:L>t\n  ?x "a" {"b"}\n'
        out, rules = soft(src)
        self.assertIn("L _>", out)
        self.assertIn('?x{"a"}{"b"}', out)
        self.assertEqual(rules, ["SE3", "SE1"])

    def test_dry_run_flag(self) -> None:
        off = dry_run([])
        self.assertEqual(off.returncode, 0, off.stderr)
        self.assertIn("soft_edge_recovery: off", off.stdout)
        on = dry_run(["--soft-edge-recovery"])
        self.assertEqual(on.returncode, 0, on.stderr)
        self.assertIn("soft_edge_recovery: on", on.stdout)


if __name__ == "__main__":
    unittest.main()
