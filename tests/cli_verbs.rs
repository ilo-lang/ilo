// CLI verb-noun subcommands: `ilo run`, `ilo check`, `ilo build`.
//
// These verbs were added in 0.12.0 alongside the modular skills work to
// match the `cargo` / `zero` / `go` toolchain conventions. They sit
// alongside the existing positional forms (`ilo file.ilo`, `ilo compile
// ...`) which remain fully supported for backwards compatibility.
//
// Tests:
//   - `ilo run file.ilo` matches `ilo file.ilo` (file + inline + args).
//   - `ilo check file.ilo` runs the verifier without executing, exit 0 on
//     clean, exit 1 on type/parse errors, supports `--json` diagnostics.
//   - `ilo build file.ilo -o out` matches `ilo compile file.ilo -o out`.
//   - `ilo run` / `ilo check` / `ilo build` with no source arg print
//     friendly usage to stderr instead of the previous "treat `run` as
//     ilo source" parser blowup.
//   - Existing positional forms (regression) still work after the dispatch
//     refactor: `ilo file.ilo`, `ilo file.ilo arg`, `ilo file.ilo func`,
//     `ilo compile file.ilo -o out`.

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

// ── ilo run ──────────────────────────────────────────────────────────────────

/// `ilo run <file>` behaves identically to `ilo <file>`.
#[test]
fn run_verb_runs_file() {
    let (_dir, path) = write_temp_ilo("main>n;42");
    let (ok, stdout, stderr) = run_args(&["run", path.to_str().unwrap()]);
    assert!(ok, "ilo run <file> should succeed; stderr: {stderr}");
    assert!(stdout.contains("42"), "stdout: {stdout}");
}

/// `ilo run <inline-code>` behaves identically to `ilo <inline-code>`.
#[test]
fn run_verb_runs_inline_code() {
    let (ok, stdout, stderr) = run_args(&["run", "main>n;42"]);
    assert!(ok, "ilo run <code> should succeed; stderr: {stderr}");
    assert!(stdout.contains("42"), "stdout: {stdout}");
}

/// `ilo run <file> arg1 arg2` forwards args to the program entry.
#[test]
fn run_verb_forwards_args() {
    let (_dir, path) = write_temp_ilo("main x:n y:n>n;+x y");
    let (ok, stdout, stderr) = run_args(&["run", path.to_str().unwrap(), "10", "32"]);
    assert!(ok, "ilo run <file> args should succeed; stderr: {stderr}");
    assert!(stdout.contains("42"), "stdout: {stdout}");
}

/// `ilo run` with no source arg prints clear usage to stderr (exit non-zero),
/// not a confusing "function header for `run` runs off end of file" parser
/// error from lexing the verb as inline ilo code.
#[test]
fn run_verb_no_args_prints_usage() {
    let (ok, _stdout, stderr) = run_args(&["run"]);
    assert!(!ok, "ilo run with no args should exit non-zero");
    assert!(
        stderr.contains("Usage: ilo run"),
        "stderr should contain usage line; got: {stderr}"
    );
    // Negative assertion: must NOT regress to the old confusing parser error.
    assert!(
        !stderr.contains("ILO-P020"),
        "should not surface the lex-as-source parser error; got: {stderr}"
    );
}

// ── ilo check ────────────────────────────────────────────────────────────────

/// `ilo check <clean-file>` exits 0 with no diagnostics.
#[test]
fn check_verb_clean_file_succeeds() {
    let (_dir, path) = write_temp_ilo("add a:n b:n>n;+a b main>n;add 1 2");
    let (ok, stdout, stderr) = run_args(&["check", path.to_str().unwrap()]);
    assert!(
        ok,
        "ilo check on a clean file should succeed; stdout: {stdout}; stderr: {stderr}"
    );
    // Check is silent on success — no diagnostics emitted.
    assert!(
        stderr.is_empty(),
        "check on clean file should produce no stderr; got: {stderr}"
    );
    assert!(
        stdout.is_empty(),
        "check on clean file should produce no stdout; got: {stdout}"
    );
}

/// `ilo check <inline-code>` works on inline source too.
#[test]
fn check_verb_inline_code() {
    let (ok, _stdout, stderr) = run_args(&["check", "main>n;42"]);
    assert!(
        ok,
        "ilo check on clean inline code should succeed; stderr: {stderr}"
    );
}

/// `ilo check <type-error-file>` exits non-zero and reports the error.
#[test]
fn check_verb_type_error_exits_nonzero() {
    // Return type mismatch: declared `>n` but body returns text.
    let (_dir, path) = write_temp_ilo("f>n;\"hello\"");
    let (ok, _stdout, stderr) = run_args(&["check", path.to_str().unwrap()]);
    assert!(
        !ok,
        "ilo check on a type-error file should exit non-zero; stderr: {stderr}"
    );
    assert!(
        !stderr.is_empty(),
        "check on broken file should emit diagnostics"
    );
}

/// `ilo check <syntactically-broken-file>` produces useful output without
/// crashing — important for editor / agent loops that may feed in
/// half-written code.
#[test]
fn check_verb_parse_error_does_not_crash() {
    // Header missing return type and body — unfinished function header.
    let (_dir, path) = write_temp_ilo("f a:n");
    let (ok, _stdout, stderr) = run_args(&["check", path.to_str().unwrap()]);
    assert!(
        !ok,
        "ilo check on a syntactically broken file should exit non-zero"
    );
    assert!(
        !stderr.is_empty(),
        "check should emit some diagnostic; stderr: {stderr}"
    );
}

/// `ilo check <file> --json` emits diagnostics as NDJSON (one JSON object
/// per line) on stderr.
#[test]
fn check_verb_json_flag_emits_json_diagnostics() {
    let (_dir, path) = write_temp_ilo("f>n;\"hello\"");
    let (ok, _stdout, stderr) = run_args(&["check", path.to_str().unwrap(), "--json"]);
    assert!(!ok);
    // First non-empty line should be valid JSON with at least a `code` or `message` field.
    let first = stderr
        .lines()
        .find(|l| !l.trim().is_empty())
        .expect("expected at least one diagnostic line");
    let parsed: serde_json::Value =
        serde_json::from_str(first).expect("check --json diagnostic should be valid JSON");
    assert!(
        parsed.get("code").is_some() || parsed.get("message").is_some(),
        "diagnostic JSON should have code/message field; got: {parsed}"
    );
}

/// `ilo check` with no source arg prints friendly usage.
#[test]
fn check_verb_no_args_prints_usage() {
    let (ok, _stdout, stderr) = run_args(&["check"]);
    assert!(!ok);
    assert!(
        stderr.contains("Usage: ilo check"),
        "stderr should contain usage line; got: {stderr}"
    );
}

// ── ilo build ────────────────────────────────────────────────────────────────

/// `ilo build <file> -o <out>` produces a binary, equivalent to
/// `ilo compile <file> -o <out>`.
///
/// Only meaningful when the cranelift feature is enabled (it's the default
/// for the release build). Skipped otherwise.
#[test]
fn build_verb_produces_binary() {
    let (dir, path) = write_temp_ilo("main>n;42");
    let out_path = dir.path().join("test-bin");
    let (ok, _stdout, stderr) = run_args(&[
        "build",
        path.to_str().unwrap(),
        "-o",
        out_path.to_str().unwrap(),
    ]);
    if !ok && stderr.contains("requires the 'cranelift' feature") {
        // Build without the cranelift feature — the dispatch is wired but
        // compilation itself is unavailable. Still validates the verb is
        // recognised.
        return;
    }
    assert!(
        ok,
        "ilo build <file> -o <out> should succeed; stderr: {stderr}"
    );
    assert!(out_path.exists(), "binary should exist at {out_path:?}");
}

/// `ilo build` with no source arg prints friendly usage.
#[test]
fn build_verb_no_args_prints_usage() {
    let (ok, _stdout, stderr) = run_args(&["build"]);
    assert!(!ok);
    assert!(
        stderr.contains("Usage: ilo build"),
        "stderr should contain usage line; got: {stderr}"
    );
}

// ── Backwards-compat regression: positional forms still work ─────────────────

/// `ilo <file.ilo>` (no verb) still runs.
#[test]
fn positional_file_still_runs() {
    let (_dir, path) = write_temp_ilo("main>n;7");
    let (ok, stdout, stderr) = run_args(&[path.to_str().unwrap()]);
    assert!(ok, "bare positional run should succeed; stderr: {stderr}");
    assert!(stdout.contains("7"));
}

/// `ilo <file.ilo> arg1 arg2` (no verb) still forwards args.
#[test]
fn positional_file_with_args_still_runs() {
    let (_dir, path) = write_temp_ilo("main x:n y:n>n;+x y");
    let (ok, stdout, stderr) = run_args(&[path.to_str().unwrap(), "3", "4"]);
    assert!(
        ok,
        "positional file + args should succeed; stderr: {stderr}"
    );
    assert!(stdout.contains("7"));
}

/// `ilo <file.ilo> func` (no verb) still selects a function.
#[test]
fn positional_file_with_func_still_runs() {
    let (_dir, path) = write_temp_ilo("dbl x:n>n;+*x 2 0 main>n;dbl 21");
    let (ok, stdout, stderr) = run_args(&[path.to_str().unwrap(), "dbl", "21"]);
    assert!(
        ok,
        "positional file + func should succeed; stderr: {stderr}"
    );
    assert!(stdout.contains("42"));
}

/// `ilo compile <file> -o <out>` (no verb) still compiles. Sister test to
/// `build_verb_produces_binary` — proves the alias didn't displace the
/// original verb.
#[test]
fn positional_compile_still_works() {
    let (dir, path) = write_temp_ilo("main>n;42");
    let out_path = dir.path().join("test-bin-compile");
    let (ok, _stdout, stderr) = run_args(&[
        "compile",
        path.to_str().unwrap(),
        "-o",
        out_path.to_str().unwrap(),
    ]);
    if !ok && stderr.contains("requires the 'cranelift' feature") {
        return;
    }
    assert!(ok, "compile should still succeed; stderr: {stderr}");
    assert!(out_path.exists());
}
