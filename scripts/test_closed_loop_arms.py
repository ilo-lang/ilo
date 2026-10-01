#!/usr/bin/env python3
"""Language-arm checks for the closed-loop harness. No API key, no network."""

from __future__ import annotations

import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SCRIPT = Path(__file__).resolve().parent / "closed-loop-bench.py"
REFS = REPO / "bench" / "closed-loop" / "references-bash"
TASKS = REPO / "bench" / "closed-loop" / "tasks.json"

spec = importlib.util.spec_from_file_location("closed_loop_bench", SCRIPT)
assert spec is not None and spec.loader is not None
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)


def _run(*args: str) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env.pop("ANTHROPIC_API_KEY", None)
    return subprocess.run(
        [sys.executable, str(SCRIPT), *args],
        capture_output=True, text=True, env=env, cwd=REPO,
    )


class ResolveArms(unittest.TestCase):
    def test_python_shorthand(self) -> None:
        arm, err = mod.resolve_lang2(True, False, None, None, None)
        self.assertIsNone(err)
        self.assertEqual(arm, mod.Lang2("python", "python3", ".py"))

    def test_python_bin_override(self) -> None:
        arm, err = mod.resolve_lang2(True, False, None, "/usr/bin/python3", ".py")
        self.assertIsNone(err)
        self.assertEqual(arm.bin, "/usr/bin/python3")
        self.assertEqual(arm.ext, ".py")

    def test_bash_shorthand_matches_long_form(self) -> None:
        short, err_s = mod.resolve_lang2(False, True, None, None, None)
        long, err_l = mod.resolve_lang2(False, False, "bash", "bash", ".sh")
        self.assertIsNone(err_s)
        self.assertIsNone(err_l)
        self.assertEqual(short, long)
        self.assertEqual(short, mod.Lang2("bash", "bash", ".sh"))

    def test_bash_name_defaults_extension(self) -> None:
        arm, err = mod.resolve_lang2(False, False, "bash", "bash", None)
        self.assertIsNone(err)
        self.assertEqual(arm.ext, ".sh")

    def test_zero_keeps_historical_extension(self) -> None:
        arm, err = mod.resolve_lang2(False, False, "zero", "zero", None)
        self.assertIsNone(err)
        self.assertEqual(arm, mod.Lang2("zero", "zero", ".zero"))

    def test_ilo_only(self) -> None:
        arm, err = mod.resolve_lang2(False, False, None, None, None)
        self.assertIsNone(arm)
        self.assertIsNone(err)

    def test_python_and_bash_conflict(self) -> None:
        arm, err = mod.resolve_lang2(True, True, None, None, None)
        self.assertIsNone(arm)
        self.assertIn("--python", err)

    def test_python_rejects_other_name(self) -> None:
        _arm, err = mod.resolve_lang2(True, False, "bash", "bash", ".sh")
        self.assertIn("python", err)

    def test_name_without_bin(self) -> None:
        _arm, err = mod.resolve_lang2(False, False, "bash", None, ".sh")
        self.assertIn("--lang2-bin", err)


class DocsAndClass(unittest.TestCase):
    def test_missing_docs_flag_is_stub(self) -> None:
        text, src, fair, err = mod.lang2_documentation("python", None)
        self.assertIsNone(err)
        self.assertEqual(src, "stub")
        self.assertFalse(fair)
        self.assertIn("No formal language documentation", text)
        self.assertIn("python", text)

    def test_missing_file_is_an_error(self) -> None:
        _text, src, fair, err = mod.lang2_documentation("bash", "/no/such/docs.md")
        self.assertEqual(src, "missing")
        self.assertFalse(fair)
        self.assertIn("not a file", err)

    def test_empty_file_is_an_error(self) -> None:
        with tempfile.NamedTemporaryFile("w", suffix=".md", delete=False) as fh:
            fh.write("   \n")
            path = fh.name
        try:
            _text, src, fair, err = mod.lang2_documentation("python", path)
        finally:
            os.unlink(path)
        self.assertEqual(src, "empty")
        self.assertFalse(fair)
        self.assertIn("empty", err)

    def test_file_is_fair_docs(self) -> None:
        with tempfile.NamedTemporaryFile("w", suffix=".md", delete=False) as fh:
            fh.write("# python\nprint is a function.\n")
            path = fh.name
        try:
            text, src, fair, err = mod.lang2_documentation("python", path)
        finally:
            os.unlink(path)
        self.assertIsNone(err)
        self.assertEqual(src, "file")
        self.assertTrue(fair)
        self.assertIn("print is a function", text)

    def test_sidecar_classes_cover_tasks_and_refs(self) -> None:
        classes = mod.load_task_classes()
        tasks = json.loads(TASKS.read_text())["tasks"]
        for task in tasks:
            self.assertIn(task["id"], classes)
            self.assertIn(classes[task["id"]], mod.VALID_TASK_CLASSES)
        self.assertEqual(classes["tool-interaction"], "ops")
        self.assertEqual(classes["data-transform"], "artefact")
        self.assertEqual(classes["simple-function"], "sanity")
        for script in REFS.glob("*.sh"):
            self.assertIn(script.stem, classes)
        self.assertEqual(len(classes), 24)

    def test_inline_class_wins(self) -> None:
        classes = {"simple-function": "sanity"}
        got = mod.task_class_of(
            {"id": "simple-function", "task_class": "artefact"}, classes,
        )
        self.assertEqual(got, "artefact")

    def test_bad_class_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "task-class.json"
            path.write_text(json.dumps({"classes": {"x": "win"}}))
            with self.assertRaises(ValueError):
                mod.load_task_classes(path)

    def test_current_tasks_name_ilo(self) -> None:
        tasks = json.loads(TASKS.read_text())["tasks"]
        named = mod.check_language_neutral(tasks, ["python"])
        self.assertEqual(named, [t["id"] for t in tasks])

    def test_neutral_text_passes(self) -> None:
        tasks = [{"id": "t", "description": "Print the triangular number of 10."}]
        self.assertEqual(mod.check_language_neutral(tasks, ["bash"]), [])

    def test_result_stem(self) -> None:
        self.assertEqual(mod.result_stem("2026-10-01", None), "closed-loop-2026-10-01")
        self.assertEqual(
            mod.result_stem("2026-10-01", "python"),
            "closed-loop-2026-10-01-python",
        )
        self.assertEqual(
            mod.result_stem("2026-10-01", "bash"),
            "closed-loop-2026-10-01-bash",
        )


class ProbeAndRun(unittest.TestCase):
    def test_python3_and_bash_probe(self) -> None:
        ok, detail = mod.probe_lang_bin("python3")
        self.assertTrue(ok, detail)
        ok, detail = mod.probe_lang_bin("bash")
        self.assertTrue(ok, detail)

    def test_dash_version_flag_falls_back(self) -> None:
        if mod.shutil.which("dash") is None:
            self.skipTest("dash not installed")
        ok, detail = mod.probe_lang_bin("dash")
        self.assertTrue(ok, detail)
        self.assertIn("version flag exited", detail)

    def test_missing_binary(self) -> None:
        ok, detail = mod.probe_lang_bin("ilo-lang-missing-binary")
        self.assertFalse(ok)
        self.assertIn("not on PATH", detail)

    def test_runner_executes_bash_and_python_files(self) -> None:
        out, err, rc = mod.run_lang2("echo arm-ok\n", "bash", ".sh")
        self.assertEqual(rc, 0, err)
        self.assertEqual(out.strip(), "arm-ok")
        out, err, rc = mod.run_lang2("print('arm-ok')\n", "python3", ".py")
        self.assertEqual(rc, 0, err)
        self.assertEqual(out.strip(), "arm-ok")

    def test_suffixed_result_files_label_the_arm(self) -> None:
        original = mod.BENCH_DIR
        try:
            with tempfile.TemporaryDirectory() as tmp:
                mod.BENCH_DIR = Path(tmp)
                cell = {
                    "task": "tool-interaction",
                    "language": "bash",
                    "lang_arm": "bash",
                    "task_class": "ops",
                    "model": "haiku",
                    "docs_source": "stub",
                    "fair_docs": False,
                    "generation_tokens": 1,
                    "input_tokens": 1,
                    "attempts_total": 1,
                    "attempts_to_success": None,
                    "repair_tokens_by_turn": [],
                    "wall_time_s": 0.1,
                    "final_outcome": "failed",
                }
                path = mod.write_json(
                    [cell], "2026-10-01", leg="bash",
                    meta={"lang_arms": ["ilo", "bash"], "fair_bakeoff": False},
                )
                self.assertEqual(path.name, "closed-loop-2026-10-01-bash.json")
                data = json.loads(path.read_text())
                self.assertEqual(data["leg"], "bash")
                self.assertEqual(data["results"][0]["lang_arm"], "bash")
                self.assertEqual(data["results"][0]["task_class"], "ops")
                self.assertIn("not an ilo manifesto loss", data["niche"])
                md = mod.write_markdown([cell], "2026-10-01", leg="bash")
                text = md.read_text()
                self.assertEqual(md.name, "closed-loop-2026-10-01-bash.md")
                self.assertIn("not a fair bakeoff", text)
                self.assertIn("not an ilo failure", text)
                self.assertIn("Language arm: bash", text)
                ilo = mod.write_json([], "2026-10-01", leg=None)
                self.assertEqual(ilo.name, "closed-loop-2026-10-01.json")
        finally:
            mod.BENCH_DIR = original


class DryRunCli(unittest.TestCase):
    def test_default_is_ilo(self) -> None:
        proc = _run("--dry-run")
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("arms: ilo\n", proc.stdout)
        self.assertIn("lang_arm: ilo ", proc.stdout)
        self.assertNotIn("lang_arm: python", proc.stdout)
        self.assertNotIn("lang_arm: bash", proc.stdout)
        self.assertIn("class=sanity", proc.stdout)
        self.assertIn("class=artefact", proc.stdout)
        self.assertIn("class=ops", proc.stdout)
        self.assertIn("language_neutral: false", proc.stdout)
        self.assertIn("not an ilo manifesto loss", proc.stdout)

    def test_python_arm(self) -> None:
        proc = _run("--dry-run", "--python")
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("arms: ilo python", proc.stdout)
        self.assertIn(
            "lang_arm: python bin=python3 ext=.py docs_source=stub fair_docs=false probe=ok",
            proc.stdout,
        )
        self.assertIn("not a fair bakeoff", proc.stdout)
        self.assertIn("docs_source=stub", proc.stderr)

    def test_bash_long_form(self) -> None:
        proc = _run(
            "--dry-run",
            "--lang2-name", "bash",
            "--lang2-bin", "bash",
            "--lang2-ext", ".sh",
        )
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("arms: ilo bash", proc.stdout)
        self.assertIn(
            "lang_arm: bash bin=bash ext=.sh docs_source=stub fair_docs=false probe=ok",
            proc.stdout,
        )

    def test_bash_shorthand(self) -> None:
        proc = _run("--dry-run", "--bash", "--task", "simple-function")
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("lang_arm: bash bin=bash ext=.sh", proc.stdout)
        self.assertIn("[simple-function] class=sanity", proc.stdout)
        self.assertNotIn("[data-transform]", proc.stdout)

    def test_docs_file_is_labelled_fair(self) -> None:
        with tempfile.NamedTemporaryFile("w", suffix=".md", delete=False) as fh:
            fh.write("Bash programs are single files.\n")
            path = fh.name
        try:
            proc = _run("--dry-run", "--bash", "--lang2-docs", path)
        finally:
            os.unlink(path)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("docs_source=file fair_docs=true probe=ok", proc.stdout)
        self.assertIn("documentation loaded", proc.stdout)
        self.assertNotIn("memorised-prior stub", proc.stdout)

    def test_missing_docs_file_exits(self) -> None:
        proc = _run("--dry-run", "--python", "--lang2-docs", "/no/such/docs.md")
        self.assertEqual(proc.returncode, 2)
        self.assertIn("not a file", proc.stderr)

    def test_require_neutral_refuses_current_tasks(self) -> None:
        proc = _run("--dry-run", "--python", "--require-language-neutral")
        self.assertEqual(proc.returncode, 2)
        self.assertIn("names a language", proc.stderr)

    def test_conflict_exits(self) -> None:
        proc = _run("--dry-run", "--python", "--bash")
        self.assertEqual(proc.returncode, 2)
        self.assertIn("only one", proc.stderr)

    def test_live_path_does_not_call_the_api_without_a_key(self) -> None:
        proc = _run("--python", "--task", "simple-function", "--model", "haiku")
        self.assertEqual(proc.returncode, 2, proc.stderr)
        self.assertNotIn("Results written", proc.stdout)
        self.assertTrue(
            "ANTHROPIC_API_KEY" in proc.stderr or "ilo binary" in proc.stderr,
            proc.stderr,
        )


class ReferenceScripts(unittest.TestCase):
    def test_every_reference_exits_zero(self) -> None:
        scripts = sorted(REFS.glob("*.sh"))
        self.assertEqual(len(scripts), 24)
        for script in scripts:
            proc = subprocess.run(
                ["bash", str(script)], capture_output=True, text=True,
            )
            self.assertEqual(proc.returncode, 0, f"{script.name}: {proc.stderr}")

    def test_main_tasks_match_expected_output(self) -> None:
        tasks = json.loads(TASKS.read_text())["tasks"]
        self.assertEqual(
            [t["id"] for t in tasks],
            [
                "simple-function",
                "with-dependencies",
                "data-transform",
                "tool-interaction",
                "workflow-rollback",
            ],
        )
        for task in tasks:
            script = REFS / f"{task['id']}.sh"
            proc = subprocess.run(
                ["bash", str(script)], capture_output=True, text=True,
            )
            expected = task["expected_output"].replace("\\n", "\n").strip()
            self.assertEqual(proc.stdout.strip(), expected, task["id"])


if __name__ == "__main__":
    unittest.main()
