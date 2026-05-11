// Tests for flexible placement of `--run-tree` / `--run-vm` / `--run-cranelift`
// / `--run-llvm` / `--run` engine flags in bare-args invocations.
//
// The flag should be accepted in any of these positions:
//   ilo <code-or-file> --run-tree [func] [args...]   (canonical)
//   ilo <code-or-file> [func] [args...] --run-tree   (trailing)
//   ilo --run-tree <code-or-file> [func] [args...]   (leading)
//
// Conflicting flags (e.g. --run-tree --run-vm) must still error out.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_args(args: &[&str]) -> (bool, String, String) {
    let out = ilo()
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn ilo: {e}"));
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    (out.status.success(), stdout, stderr)
}

fn write_temp_ilo(content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("test.ilo");
    std::fs::write(&path, content).expect("write temp ilo");
    (dir, path)
}

// ── inline code, --run-tree in every position ─────────────────────────────────

#[test]
fn run_tree_flag_after_code_before_func() {
    let (ok, stdout, stderr) = run_args(&["f>n;5", "--run-tree", "f"]);
    assert!(ok, "canonical placement should work; stderr: {stderr}");
    assert_eq!(stdout.trim(), "5");
}

#[test]
fn run_tree_flag_after_func() {
    let (ok, stdout, stderr) = run_args(&["f>n;5", "f", "--run-tree"]);
    assert!(ok, "trailing --run-tree should work; stderr: {stderr}");
    assert_eq!(stdout.trim(), "5");
}

#[test]
fn run_tree_flag_before_code() {
    let (ok, stdout, stderr) = run_args(&["--run-tree", "f>n;5", "f"]);
    assert!(ok, "leading --run-tree should work; stderr: {stderr}");
    assert_eq!(stdout.trim(), "5");
}

// ── --run-vm in every position ────────────────────────────────────────────────

#[test]
fn run_vm_flag_after_code_before_func() {
    let (ok, stdout, stderr) = run_args(&["f x:n>n;*x 2", "--run-vm", "f", "5"]);
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "10");
}

#[test]
fn run_vm_flag_after_func() {
    let (ok, stdout, stderr) = run_args(&["f x:n>n;*x 2", "f", "5", "--run-vm"]);
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "10");
}

#[test]
fn run_vm_flag_before_code() {
    let (ok, stdout, stderr) = run_args(&["--run-vm", "f x:n>n;*x 2", "f", "5"]);
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "10");
}

// ── --run-cranelift in every position ─────────────────────────────────────────
// Cranelift may not be available in every build profile; just check that the
// flag is accepted (either succeeds, or fails for cranelift-specific reasons
// rather than "unknown flag" / argument-order parse errors).

#[test]
fn run_cranelift_flag_after_func_accepted() {
    let (_ok, _stdout, stderr) = run_args(&["f>n;5", "f", "--run-cranelift"]);
    assert!(
        !stderr.contains("Usage:") && !stderr.contains("unknown flag"),
        "--run-cranelift trailing should be parsed; stderr: {stderr}"
    );
}

#[test]
fn run_cranelift_flag_before_code_accepted() {
    let (_ok, _stdout, stderr) = run_args(&["--run-cranelift", "f>n;5", "f"]);
    assert!(
        !stderr.contains("Usage:") && !stderr.contains("unknown flag"),
        "--run-cranelift leading should be parsed; stderr: {stderr}"
    );
}

// ── file path: --run-tree before and after the filename ───────────────────────

#[test]
fn run_tree_flag_after_file_path() {
    let (_dir, path) = write_temp_ilo("main>n;5");
    let (ok, stdout, stderr) = run_args(&[path.to_str().unwrap(), "--run-tree", "main"]);
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "5");
}

#[test]
fn run_tree_flag_before_file_path() {
    let (_dir, path) = write_temp_ilo("main>n;5");
    let (ok, stdout, stderr) = run_args(&["--run-tree", path.to_str().unwrap(), "main"]);
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "5");
}

#[test]
fn run_tree_flag_trailing_after_file_and_func() {
    let (_dir, path) = write_temp_ilo("main>n;5");
    let (ok, stdout, stderr) = run_args(&[path.to_str().unwrap(), "main", "--run-tree"]);
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "5");
}

// ── conflict detection still fires ────────────────────────────────────────────

#[test]
fn conflicting_run_flags_error() {
    let (ok, _stdout, stderr) = run_args(&["--run-tree", "--run-vm", "f>n;5", "f"]);
    assert!(!ok, "conflicting engine flags should error");
    assert!(
        stderr.contains("mutually exclusive"),
        "should mention mutual exclusion; stderr: {stderr}"
    );
}

#[test]
fn conflicting_run_flags_trailing_error() {
    let (ok, _stdout, stderr) = run_args(&["f>n;5", "f", "--run-tree", "--run-vm"]);
    assert!(!ok, "conflicting engine flags should error");
    assert!(
        stderr.contains("mutually exclusive"),
        "should mention mutual exclusion; stderr: {stderr}"
    );
}

// ── repeated same flag is fine ────────────────────────────────────────────────

#[test]
fn repeated_same_run_flag_ok() {
    let (ok, stdout, stderr) = run_args(&["--run-tree", "f>n;5", "f", "--run-tree"]);
    assert!(
        ok,
        "repeating the same engine flag should not conflict; stderr: {stderr}"
    );
    assert_eq!(stdout.trim(), "5");
}

// ── --run alias (synonym for --run-tree) ──────────────────────────────────────

#[test]
fn run_alias_trailing() {
    let (ok, stdout, stderr) = run_args(&["f>n;5", "f", "--run"]);
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "5");
}

#[test]
fn run_alias_leading() {
    let (ok, stdout, stderr) = run_args(&["--run", "f>n;5", "f"]);
    assert!(ok, "stderr: {stderr}");
    assert_eq!(stdout.trim(), "5");
}

// ── canonical clap subcommand path still works ────────────────────────────────

#[test]
fn run_subcommand_path_unchanged() {
    let (ok, stdout, stderr) = run_args(&["run", "--run-tree", "f>n;5", "f"]);
    assert!(
        ok,
        "clap subcommand path should still work; stderr: {stderr}"
    );
    assert_eq!(stdout.trim(), "5");
}
