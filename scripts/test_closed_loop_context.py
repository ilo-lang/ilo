#!/usr/bin/env python3
"""Context-arm checks for closed-loop-bench. No API key and no ilo binary."""

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
TASKS = REPO_ROOT / "bench" / "closed-loop" / "tasks.json"


def load_harness():
    spec = importlib.util.spec_from_file_location("closed_loop_bench", SCRIPT)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {SCRIPT}")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


H = load_harness()


def dry_run(extra: list[str]) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env.pop("ANTHROPIC_API_KEY", None)
    return subprocess.run(
        [sys.executable, str(SCRIPT), "--dry-run", *extra],
        cwd=REPO_ROOT,
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )


def modules_of(stdout: str, task_id: str) -> list[str]:
    """Return the modules line printed for one task."""
    needle = f"[{task_id}]"
    lines = stdout.splitlines()
    for i, line in enumerate(lines):
        if needle in line:
            for follow in lines[i + 1 : i + 6]:
                stripped = follow.strip()
                if stripped.startswith("modules:"):
                    return stripped.split(":", 1)[1].split()
    raise AssertionError(f"no modules line for {task_id} in:\n{stdout}")


class ContextModulesTest(unittest.TestCase):
    def test_fixed_arms_match_plan_lists(self) -> None:
        self.assertEqual(
            H.modules_for("core"),
            ["ilo-language", "ilo-builtins-core"],
        )
        self.assertEqual(
            H.modules_for("curated"),
            ["ilo-language", "ilo-builtins-core", "ilo-builtins-sig"],
        )
        self.assertEqual(
            H.modules_for("full"),
            [
                "ilo-language",
                "ilo-builtins-core",
                "ilo-builtins-sig",
                "ilo-builtins-io",
                "ilo-builtins-text",
                "ilo-builtins-math",
            ],
        )

    def test_loaded_text_grows_core_curated_full(self) -> None:
        core = H.ilo_context("ilo", "core")
        curated = H.ilo_context("ilo", "curated")
        full = H.ilo_context("ilo", "full")
        self.assertLess(len(core), len(curated))
        self.assertLess(len(curated), len(full))
        self.assertIn("ilo-language", core)
        self.assertIn("ilo-builtins-sig", curated)
        self.assertNotIn("ilo-builtins-sig", core)

    def test_task_modules_equal_declared_list(self) -> None:
        tasks = json.loads(TASKS.read_text())["tasks"]
        self.assertGreaterEqual(len(tasks), 1)
        for task in tasks:
            loaded = H.modules_for("task-modules", task)
            self.assertEqual(loaded, task["modules"])
            self.assertTrue(set(loaded) <= set(task["modules"]))

    def test_task_modules_missing_list_errors(self) -> None:
        with self.assertRaises(H.ContextError):
            H.modules_for("task-modules", {"id": "no-modules"})
        with self.assertRaises(H.ContextError):
            H.modules_for("task-modules", {"id": "empty", "modules": []})

    def test_modules_from_task_alias(self) -> None:
        self.assertEqual(H.resolve_context_mode(None, False), "curated")
        self.assertEqual(H.resolve_context_mode(None, True), "task-modules")
        self.assertEqual(
            H.resolve_context_mode("task-modules", True), "task-modules",
        )
        with self.assertRaises(H.ContextError):
            H.resolve_context_mode("core", True)

    def test_unknown_module_is_not_a_stub(self) -> None:
        with self.assertRaises(H.ContextError):
            H.load_skill_text("not-a-real-module", "ilo")


class DryRunTest(unittest.TestCase):
    def test_each_arm_labels_and_lists_modules(self) -> None:
        expected = {
            "core": ["ilo-language", "ilo-builtins-core"],
            "curated": [
                "ilo-language", "ilo-builtins-core", "ilo-builtins-sig",
            ],
            "full": [
                "ilo-language",
                "ilo-builtins-core",
                "ilo-builtins-sig",
                "ilo-builtins-io",
                "ilo-builtins-text",
                "ilo-builtins-math",
            ],
        }
        chars: dict[str, int] = {}
        for arm, mods in expected.items():
            proc = dry_run(["--context", arm, "--task", "simple-function"])
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertNotIn("ANTHROPIC_API_KEY", proc.stderr)
            self.assertIn(f"context: {arm}", proc.stdout)
            self.assertEqual(modules_of(proc.stdout, "simple-function"), mods)
            line = next(
                ln for ln in proc.stdout.splitlines()
                if ln.strip().startswith("context_chars:")
            )
            chars[arm] = int(line.split(":", 1)[1].strip())
        self.assertLess(chars["core"], chars["curated"])
        self.assertLess(chars["curated"], chars["full"])

    def test_task_modules_differ_by_task(self) -> None:
        proc = dry_run(["--context", "task-modules"])
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("context: task-modules", proc.stdout)
        simple = modules_of(proc.stdout, "simple-function")
        tool = modules_of(proc.stdout, "tool-interaction")
        self.assertEqual(simple, ["ilo-language", "ilo-builtins-math"])
        self.assertEqual(tool, ["ilo-language", "ilo-builtins-io"])
        self.assertNotEqual(simple, tool)

    def test_modules_from_task_flag(self) -> None:
        proc = dry_run(["--modules-from-task", "--task", "data-transform"])
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(
            modules_of(proc.stdout, "data-transform"),
            ["ilo-language", "ilo-builtins-core", "ilo-builtins-math"],
        )

    def test_conflicting_flags(self) -> None:
        proc = dry_run(["--context", "full", "--modules-from-task"])
        self.assertEqual(proc.returncode, 2)
        self.assertIn("task-modules", proc.stderr)

    def test_default_dry_run_is_curated(self) -> None:
        proc = dry_run([])
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("context: curated", proc.stdout)
        self.assertIn("ilo-builtins-sig", proc.stdout)

    def test_writers_label_every_cell(self) -> None:
        cell = {
            "task": "simple-function",
            "language": "ilo",
            "model": "haiku",
            "context": "core",
            "context_modules": ["ilo-language", "ilo-builtins-core"],
            "generation_tokens": 0,
            "input_tokens": 0,
            "repair_tokens_by_turn": [],
            "attempts_to_success": None,
            "attempts_total": 0,
            "success_rate": 0.0,
            "wall_time_s": 0.0,
            "final_outcome": "failed",
        }
        previous = H.BENCH_DIR
        with tempfile.TemporaryDirectory() as tmp:
            H.BENCH_DIR = Path(tmp)
            try:
                json_path = H.write_json([cell], "2099-01-01", "core")
                md_path = H.write_markdown([cell], "2099-01-01", "core")
                payload = json.loads(json_path.read_text())
                md = md_path.read_text()
            finally:
                H.BENCH_DIR = previous
        self.assertEqual(payload["context"], "core")
        self.assertEqual(payload["results"][0]["context"], "core")
        self.assertEqual(
            payload["results"][0]["context_modules"],
            ["ilo-language", "ilo-builtins-core"],
        )
        self.assertIn("Context arm: core", md)
        self.assertIn("ilo-language, ilo-builtins-core", md)


if __name__ == "__main__":
    unittest.main()
