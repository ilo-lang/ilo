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
ARTEFACT = REPO_ROOT / "bench" / "closed-loop" / "tasks-artefact-exp07.json"
CONTEXT = REPO_ROOT / "bench" / "closed-loop" / "context"
CARD = CONTEXT / "one-shape-conditional.md"
RUN_NOTE = REPO_ROOT / "bench" / "closed-loop" / "arm-c-run-note.md"

SHAPE_LINES = (
    "Conditional (only): `?h cond then else`. Example: `m=?h >ov 0 ov 0`. "
    "Nested else: `(?h cond then else)`.",
    "Non-canonical here: `?h a b`, `?h{then}{else}`, `?cond{then}{else}`, "
    "match `?x{...}`, braceless guard.",
)
LOOP_LINE = "Loop: `@name xs{body}` — no `in`, not a decorator."


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


def artefact_tasks() -> list[dict]:
    return json.loads(ARTEFACT.read_text())["tasks"]


class TaskOneshapeTest(unittest.TestCase):
    def test_arm_resolves_and_rejects_alias_mix(self) -> None:
        self.assertEqual(
            H.resolve_context_mode("task-oneshape", False), "task-oneshape",
        )
        with self.assertRaises(H.ContextError):
            H.resolve_context_mode("task-oneshape", True)

    def test_artefact_modules_stay_arm_b(self) -> None:
        tasks = artefact_tasks()
        self.assertEqual(
            [t["id"] for t in tasks],
            [
                "csv-sales-summary",
                "record-normalize",
                "schedule-window",
                "typed-rollup",
            ],
        )
        for task in tasks:
            self.assertEqual(H.modules_for("task-modules", task), task["modules"])
            self.assertEqual(
                H.modules_for("task-oneshape", task), task["oneshape_modules"],
            )
            self.assertNotEqual(task["modules"], task["oneshape_modules"])

    def test_missing_oneshape_list_errors(self) -> None:
        with self.assertRaises(H.ContextError):
            H.modules_for("task-oneshape", {"id": "no-oneshape", "modules": ["x"]})
        with self.assertRaises(H.ContextError):
            H.modules_for(
                "task-oneshape", {"id": "empty", "oneshape_modules": []},
            )
        with self.assertRaises(H.ContextError):
            H.modules_for("task-oneshape", None)

    def test_shape_lines_match_card_byte_for_byte(self) -> None:
        card = CARD.read_text().splitlines()
        for line in SHAPE_LINES:
            self.assertEqual(card.count(line), 1)
        for name in (
            "ilo-excerpt-csv-oneshape",
            "ilo-excerpt-record-oneshape",
            "ilo-excerpt-schedule-oneshape",
            "ilo-excerpt-typed-oneshape",
        ):
            lines = (CONTEXT / f"{name}.md").read_text().splitlines()
            for line in SHAPE_LINES:
                self.assertEqual(lines.count(line), 1, name)
        for name in ("ilo-excerpt-record-oneshape", "ilo-excerpt-typed-oneshape"):
            lines = (CONTEXT / f"{name}.md").read_text().splitlines()
            self.assertEqual(lines.count(LOOP_LINE), 1, name)

    def test_oneshape_does_not_teach_a_second_conditional(self) -> None:
        banned = ("?h{", "?cond{", "?x{", "early-return", "true:", "false:")
        for name in (
            "ilo-excerpt-csv-oneshape",
            "ilo-excerpt-record-oneshape",
            "ilo-excerpt-schedule-oneshape",
            "ilo-excerpt-typed-oneshape",
        ):
            text = (CONTEXT / f"{name}.md").read_text()
            kept = text.replace(SHAPE_LINES[1], "")
            for token in banned:
                self.assertNotIn(token, kept, f"{name} teaches {token}")
            self.assertNotIn("?h a b", kept)

    def test_oneshape_stays_in_arm_b_band(self) -> None:
        for task in artefact_tasks():
            arm_b = H.ilo_context("ilo", "task-modules", task)
            arm_c = H.ilo_context("ilo", "task-oneshape", task)
            self.assertLess(len(arm_c), 1200, task["id"])
            self.assertLess(len(arm_c), int(len(arm_b) * 1.5) + 1, task["id"])
            self.assertIn(SHAPE_LINES[0], arm_c)
            self.assertIn(SHAPE_LINES[1], arm_c)

    def test_run_note_records_card_sha(self) -> None:
        import hashlib
        digest = hashlib.sha256(CARD.read_bytes()).hexdigest()
        note = RUN_NOTE.read_text()
        self.assertIn(digest, note)
        self.assertIn("7b15b28dac876f977d5280387e6ae09c5bb56b8f", note)
        self.assertLessEqual(
            len(CARD.read_text().splitlines()), 40,
        )

    def test_dry_run_artefact_oneshape(self) -> None:
        proc = dry_run([
            "--task-set", "artefact-exp07", "--context", "task-oneshape",
        ])
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("context: task-oneshape", proc.stdout)
        self.assertNotIn("ANTHROPIC_API_KEY", proc.stderr)
        expected = {
            "csv-sales-summary": ["ilo-excerpt-csv-oneshape"],
            "record-normalize": ["ilo-excerpt-record-oneshape"],
            "schedule-window": ["ilo-excerpt-schedule-oneshape"],
            "typed-rollup": ["ilo-excerpt-typed-oneshape"],
        }
        chars = []
        for task_id, mods in expected.items():
            self.assertEqual(modules_of(proc.stdout, task_id), mods)
            block = proc.stdout.split(f"[{task_id}]", 1)[1]
            line = next(
                ln for ln in block.splitlines()
                if ln.strip().startswith("context_chars:")
            )
            chars.append(int(line.split(":", 1)[1].strip()))
        self.assertTrue(all(n < 1200 for n in chars))
        arm_b = dry_run([
            "--task-set", "artefact-exp07", "--context", "task-modules",
        ])
        self.assertEqual(arm_b.returncode, 0, arm_b.stderr)
        self.assertEqual(
            modules_of(arm_b.stdout, "schedule-window"),
            ["ilo-excerpt-schedule"],
        )


if __name__ == "__main__":
    unittest.main()
