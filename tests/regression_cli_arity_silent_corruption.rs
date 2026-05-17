// Regression coverage for the v0.11.6 P0 silent-corruption regression where
// the VM and Cranelift JIT silently nil-padded missing CLI args instead of
// erroring (interactive-cli rerun7).
//
// Reproduces: `ilo main.ilo add` on a tracker script that declared
// `add txt:t>R t t;...` returned exit 0 and wrote the literal string
// "[ ] nil" to tasks.txt. v0.11.5 errored correctly via the tree
// interpreter's arity guard; PR #336's listview reshape made
// `JitCallError::NotEligible` fall through to `vm::run`, and the VM's
// `setup_call` happily pre-allocated registers with NanVal::nil(),
// turning the missing arg into a silent `nil` binding.
//
// Coverage matrix:
//   - sub-arity (missing required positional)
//   - super-arity (extra positional)
//   - happy path (exact arity) — unchanged behaviour
//   - every engine (default, --run-tree, --run-vm, --run-cranelift)
//   - inline (`ilo 'src' ...`) and file (`ilo main.ilo ...`)
//   - auto-main file dispatch (`ilo main.ilo` with main taking args)
//
// The contract is "strict arity, every engine, loud ILO-R004". Sub-arity
// must never coerce to nil. Super-arity must never silently drop extras.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_args(args: &[&str]) -> (i32, String, String) {
    let out = ilo()
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn ilo: {e}"));
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    (code, stdout, stderr)
}

fn assert_arity_error(out: (i32, String, String), expected_fn: &str, want: usize, got: usize) {
    let (code, stdout, stderr) = out;
    assert_eq!(
        code, 1,
        "expected exit 1 for arity mismatch; stdout=\n{stdout}\nstderr=\n{stderr}"
    );
    let needle = format!("{}: expected {} args, got {}", expected_fn, want, got);
    let combined = format!("{stdout}\n{stderr}");
    assert!(
        combined.contains(&needle) && combined.contains("ILO-R004"),
        "expected `{needle}` + ILO-R004 in output; got:\n{combined}"
    );
}

// ── inline sub-arity (missing required positional) ────────────────────────────

#[test]
fn inline_sub_arity_default_engine() {
    // The originating bug. Pre-fix: prints `nil` exit 0.
    assert_arity_error(run_args(&["f x:n>n;+x 1"]), "f", 1, 0);
}

#[test]
fn inline_sub_arity_run_tree() {
    assert_arity_error(run_args(&["--run-tree", "f x:n>n;+x 1"]), "f", 1, 0);
}

#[test]
fn inline_sub_arity_run_vm() {
    // Pre-fix: prints `nil` exit 0. VM setup_call padded with NanVal::nil().
    assert_arity_error(run_args(&["--run-vm", "f x:n>n;+x 1"]), "f", 1, 0);
}

#[test]
fn inline_sub_arity_run_cranelift() {
    // Pre-fix: JIT bailed via NotEligible, fell through to nil-padded VM.
    assert_arity_error(run_args(&["--run-cranelift", "f x:n>n;+x 1"]), "f", 1, 0);
}

// ── inline super-arity (extra positional) ─────────────────────────────────────
//
// Only the default-engine path can faithfully detect super-arity for inline
// snippets — the explicit `--run-*` engines treat positionals after the
// source as `[func, args...]` and surface ILO-R002 "undefined function"
// when the next positional isn't a known function. That pre-existing
// shape is out of scope for this regression; the silent-corruption fix
// is specifically about the auto-resolved default-engine path.

#[test]
fn inline_super_arity_default_engine() {
    assert_arity_error(run_args(&["f x:n>n;+x 1", "5", "7"]), "f", 1, 2);
}

// ── inline happy path (exact arity) ───────────────────────────────────────────

#[test]
fn inline_exact_arity_default_engine() {
    let (code, stdout, _) = run_args(&["f x:n>n;+x 1", "5"]);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "6");
}

#[test]
fn inline_exact_arity_run_vm() {
    let (code, stdout, _) = run_args(&["--run-vm", "f x:n>n;+x 1", "f", "5"]);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "6");
}

#[test]
fn inline_exact_arity_run_cranelift() {
    let (code, stdout, _) = run_args(&["--run-cranelift", "f x:n>n;+x 1", "f", "5"]);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "6");
}

#[test]
fn inline_exact_arity_run_tree() {
    let (code, stdout, _) = run_args(&["--run-tree", "f x:n>n;+x 1", "f", "5"]);
    assert_eq!(code, 0);
    assert_eq!(stdout.trim(), "6");
}

// ── file dispatch with auto-main routing ──────────────────────────────────────
//
// The interactive-cli tracker is a multi-function file that routes
// subcommands through `main cmd:t arg:t>...`. Pre-fix:
//   `ilo main.ilo add` resolved entry to `main`, parsed CLI as `["add"]`
//   (one positional), then VM `setup_call` padded `arg` with nil. The
//   `?cmd{"add":add arg;...}` body then evaluated `add nil` -> wrote
//   `[ ] nil` to tasks.txt silently.
// Post-fix: ILO-R004 fires at the CLI boundary because the resolved
// entry `main` declares 2 params but only 1 was supplied.

fn write_tracker(dir: &std::path::Path) -> std::path::PathBuf {
    // Minimal echo of the interactive-cli rerun7 tracker shape: a `main`
    // that routes a subcommand to a function which writes a tagged line
    // to disk via `wrl`. Keeps the silent-corruption surface (file write
    // + nil-coerced second positional) intact while avoiding load/save
    // round-trip complexity.
    let src = r#"add txt:t>R t t;ln=fmt "[ ] {}" txt;wrl "tasks.txt" [ln]
main cmd:t arg:t>R t t;?cmd{"add":add arg;_:^"usage"}
"#;
    let path = dir.join("tracker.ilo");
    std::fs::write(&path, src).expect("write tracker");
    path
}

#[test]
fn file_auto_main_sub_arity_default() {
    // `ilo tracker.ilo add` — `add` is a declared function, so the
    // default-engine CLI routes directly to `add txt:t` with no positional
    // args. Pre-fix: VM nil-padded `txt`, ran `wrl "tasks.txt" ["[ ] nil"]`,
    // silently wrote corrupt data to disk and exited 0. Post-fix: the
    // CLI-boundary arity guard reports `add: expected 1 args, got 0` and
    // exits 1 before any side effects.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_tracker(dir.path());
    let out = ilo()
        .current_dir(dir.path())
        .args([path.to_str().unwrap(), "add"])
        .output()
        .expect("spawn");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert_arity_error((code, stdout, stderr), "add", 1, 0);
    // Crucially: the broken behaviour wrote a `[ ] nil` line. Tasks.txt
    // must NOT exist after the dispatch failed.
    assert!(
        !dir.path().join("tasks.txt").exists(),
        "tasks.txt must NOT be written when arity check fires"
    );
}

#[test]
fn file_auto_main_no_positional_routes_to_main() {
    // Bare `ilo tracker.ilo` (no positionals) auto-runs `main`. Main
    // declares 2 params (cmd, arg) — supply none -> the CLI guard
    // reports `main: expected 2 args, got 0`. Pinning this shape
    // because the auto-pick-main heuristic (#329) is the other path
    // that could nil-pad without our guard.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_tracker(dir.path());
    let out = ilo()
        .current_dir(dir.path())
        .args([path.to_str().unwrap()])
        .output()
        .expect("spawn");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert_arity_error((code, stdout, stderr), "main", 2, 0);
    assert!(!dir.path().join("tasks.txt").exists());
}

#[test]
fn file_auto_main_exact_arity_writes_task() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_tracker(dir.path());
    let out = ilo()
        .current_dir(dir.path())
        .args([path.to_str().unwrap(), "add", "buy milk"])
        .output()
        .expect("spawn");
    assert_eq!(
        out.status.code().unwrap_or(-1),
        0,
        "happy path should succeed; stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let written = std::fs::read_to_string(dir.path().join("tasks.txt")).expect("tasks.txt");
    assert!(
        written.contains("[ ] buy milk"),
        "expected tasks.txt to contain `[ ] buy milk`; got {written:?}"
    );
    // And critically: no `nil`.
    assert!(
        !written.contains("nil"),
        "tasks.txt must never contain `nil` (regression marker); got {written:?}"
    );
}

#[test]
fn file_auto_main_super_arity_default() {
    // `ilo tracker.ilo add "buy milk" extra` — routes to `add txt:t` with
    // 2 positional args. Pre-fix: VM ignored the extra and wrote `[ ] buy
    // milk` (extras silently dropped — the second half of the rerun7
    // report). Post-fix: ILO-R004 fires with `add: expected 1 args, got 2`.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_tracker(dir.path());
    let out = ilo()
        .current_dir(dir.path())
        .args([path.to_str().unwrap(), "add", "buy milk", "extra"])
        .output()
        .expect("spawn");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert_arity_error((code, stdout, stderr), "add", 1, 2);
    assert!(
        !dir.path().join("tasks.txt").exists(),
        "tasks.txt must NOT be written when arity check fires"
    );
}

// ── explicit-engine file dispatch ─────────────────────────────────────────────
//
// The `--run-vm` / `--run-cranelift` engine flag with a file routes
// non-ident first positional to main (per the #329 / #336 fix). Cover
// that the arity guard fires regardless of engine.

#[test]
fn file_main_sub_arity_run_vm() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = "main x:n y:n>n;+x y\n";
    let path = dir.path().join("two.ilo");
    std::fs::write(&path, src).expect("write");
    // --run-vm with file + 1 positional that LOOKS like an ident routes
    // to the named function path (engine resolves `main` because no
    // positional is given). Provide one ambiguous positional later.
    let out = ilo()
        .args(["--run-vm", path.to_str().unwrap()])
        .output()
        .expect("spawn");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert_arity_error((code, stdout, stderr), "main", 2, 0);
}

#[test]
fn file_main_sub_arity_run_cranelift() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = "main x:n y:n>n;+x y\n";
    let path = dir.path().join("two.ilo");
    std::fs::write(&path, src).expect("write");
    let out = ilo()
        .args(["--run-cranelift", path.to_str().unwrap()])
        .output()
        .expect("spawn");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert_arity_error((code, stdout, stderr), "main", 2, 0);
}

#[test]
fn file_main_sub_arity_run_tree() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = "main x:n y:n>n;+x y\n";
    let path = dir.path().join("two.ilo");
    std::fs::write(&path, src).expect("write");
    let out = ilo()
        .args(["--run-tree", path.to_str().unwrap()])
        .output()
        .expect("spawn");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert_arity_error((code, stdout, stderr), "main", 2, 0);
}

#[test]
fn file_main_exact_arity_run_vm() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = "main x:n y:n>n;+x y\n";
    let path = dir.path().join("two.ilo");
    std::fs::write(&path, src).expect("write");
    let out = ilo()
        .args(["--run-vm", path.to_str().unwrap(), "main", "3", "4"])
        .output()
        .expect("spawn");
    assert_eq!(out.status.code().unwrap_or(-1), 0);
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "7");
}
