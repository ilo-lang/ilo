// ILO-442: `-j` short alias for `--json` on every CLI subcommand.
//
// Manifesto P6 promises every subcommand has `--json`. This file pins
// the parallel `-j` short alias on every subcommand that accepts
// `--json`, plus a no-regression test that the long form still works.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(args: &[&str]) -> (i32, String, String) {
    let out = ilo()
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn ilo: {e}"));
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn check_short_j_emits_json_diagnostic() {
    // Broken source so the verifier emits at least one diagnostic.
    let (code, stdout, stderr) = run(&["check", "-j", "fn"]);
    assert_eq!(code, 1, "stdout={stdout} stderr={stderr}");
    // JSON diagnostic goes to stderr in NDJSON mode; check both for the
    // leading brace so we're not coupling to which stream it lands on.
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("\"code\""),
        "expected JSON diagnostic, got: {combined}"
    );
}

#[test]
fn check_long_json_still_works() {
    // No-regression: --json must keep producing JSON.
    let (code, stdout, stderr) = run(&["check", "--json", "fn"]);
    assert_eq!(code, 1);
    let combined = format!("{stdout}{stderr}");
    assert!(combined.contains("\"code\""));
}

#[test]
fn run_short_j_wraps_value() {
    // `ilo run -j 'code' arg` should wrap with schemaVersion envelope.
    let (code, stdout, _stderr) = run(&["run", "-j", "f x:n>n;*x 2", "5"]);
    assert_eq!(code, 0, "stdout={stdout}");
    assert!(stdout.contains("\"schemaVersion\""), "stdout={stdout}");
    assert!(stdout.contains("\"ok\""), "stdout={stdout}");
}

#[test]
fn spec_short_j_ai_emits_json_envelope() {
    let (code, stdout, _stderr) = run(&["spec", "-j", "ai"]);
    assert_eq!(code, 0);
    assert!(
        stdout.starts_with("{"),
        "stdout={}",
        &stdout[..stdout.len().min(80)]
    );
    assert!(
        stdout.contains("\"builtins\""),
        "spec -j ai should include builtins"
    );
    assert!(stdout.contains("\"schemaVersion\""));
}

#[test]
fn version_short_j_emits_json() {
    let (code, stdout, _) = run(&["version", "-j"]);
    assert_eq!(code, 0);
    assert!(stdout.starts_with("{"), "stdout={stdout}");
    assert!(stdout.contains("\"features\""));
}

#[test]
fn explain_short_j_emits_json() {
    let (code, stdout, _) = run(&["explain", "-j", "ILO-T001"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("\"code\""));
    assert!(stdout.contains("ILO-T001"));
}

#[test]
fn skill_list_short_j_emits_json() {
    let (code, stdout, _) = run(&["skill", "-j", "list"]);
    assert_eq!(code, 0);
    assert!(stdout.contains("\"schemaVersion\""));
    assert!(stdout.contains("\"skills\""));
}
