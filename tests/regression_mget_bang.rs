// Regression tests for `!` auto-unwrap on Optional-returning builtins.
//
// Background: until this change, `mget!` (and any other O-returning builtin)
// was rejected by the verifier with ILO-T025 because `!` only accepted callees
// whose return type was `R _ _`. Every consumer of Optional ended up writing
// the same two-step `r=mget m k;v=r??default` bind to extract a value, even
// when the key was known to be present and nil-propagation was the desired
// behaviour on miss.
//
// `!` on an O-returning call is now defined as:
//   - if the result is `Some(v)` (i.e. non-nil at runtime), the call yields v
//   - if the result is nil, propagate nil as the enclosing function's return
//     (parallel to how `!` on `R _ _` propagates `Err`)
//
// The verifier requires the enclosing function's return type to accept nil
// (Optional or Unknown); otherwise it emits ILO-T026.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} unexpectedly succeeded for `{src}`: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

// ── present key: mget! returns the inner value ───────────────────────────
const PRESENT_SRC: &str = r#"f>O n;m=mset mmap "k" 5;v=mget! m "k";v"#;

fn check_present(engine: &str) {
    assert_eq!(run(engine, PRESENT_SRC, "f"), "5", "engine={engine}");
}

#[test]
fn mget_bang_present_tree() {
    check_present("--run-tree");
}

#[test]
fn mget_bang_present_vm() {
    check_present("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn mget_bang_present_cranelift() {
    check_present("--run-cranelift");
}

// ── missing key: mget! propagates nil out of the enclosing function ──────
const MISSING_SRC: &str = r#"f>O n;m=mmap;v=mget! m "missing";v"#;

fn check_missing(engine: &str) {
    assert_eq!(run(engine, MISSING_SRC, "f"), "nil", "engine={engine}");
}

#[test]
fn mget_bang_missing_tree() {
    check_missing("--run-tree");
}

#[test]
fn mget_bang_missing_vm() {
    check_missing("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn mget_bang_missing_cranelift() {
    check_missing("--run-cranelift");
}

// ── mget! propagation short-circuits subsequent statements ───────────────
// If mget! propagates nil from line 2, the `99` on line 3 must not execute.
const SHORTCIRCUIT_SRC: &str = r#"f>O n;m=mmap;v=mget! m "k";+v 99"#;

fn check_shortcircuit(engine: &str) {
    assert_eq!(run(engine, SHORTCIRCUIT_SRC, "f"), "nil", "engine={engine}");
}

#[test]
fn mget_bang_shortcircuit_tree() {
    check_shortcircuit("--run-tree");
}

#[test]
fn mget_bang_shortcircuit_vm() {
    check_shortcircuit("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn mget_bang_shortcircuit_cranelift() {
    check_shortcircuit("--run-cranelift");
}

// ── verifier rejects mget! in a non-Optional-returning function ──────────
// `f>n;...mget! m "k"` declares a numeric return; nil can't flow through it.
// ILO-T026 is the same diagnostic used for Result mismatch — the message
// distinguishes "not a Result" vs "not an Optional".
#[test]
fn mget_bang_in_non_optional_fn_rejected() {
    let stderr = run_err("--run-vm", r#"f>n;m=mmap;mget! m "x""#, "f");
    assert!(
        stderr.contains("ILO-T026") && stderr.contains("Optional"),
        "expected ILO-T026 mentioning Optional, got: {stderr}"
    );
}

// ── two-step nil-coalesce idiom still works (no regression) ──────────────
const TWO_STEP_SRC: &str = r#"f>n;m=mset mmap "k" 5;r=mget m "k";v=r??0;v"#;

fn check_two_step(engine: &str) {
    assert_eq!(run(engine, TWO_STEP_SRC, "f"), "5", "engine={engine}");
}

#[test]
fn mget_two_step_default_tree() {
    check_two_step("--run-tree");
}

#[test]
fn mget_two_step_default_vm() {
    check_two_step("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn mget_two_step_default_cranelift() {
    check_two_step("--run-cranelift");
}

// Two-step on missing key uses the default.
const TWO_STEP_MISS_SRC: &str = r#"f>n;m=mmap;r=mget m "k";v=r??42;v"#;

fn check_two_step_miss(engine: &str) {
    assert_eq!(run(engine, TWO_STEP_MISS_SRC, "f"), "42", "engine={engine}");
}

#[test]
fn mget_two_step_default_miss_tree() {
    check_two_step_miss("--run-tree");
}

#[test]
fn mget_two_step_default_miss_vm() {
    check_two_step_miss("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn mget_two_step_default_miss_cranelift() {
    check_two_step_miss("--run-cranelift");
}

// ── existing Result `!` path unchanged ───────────────────────────────────
// num! returns R n t; propagation should yield the wrapped Err.
const RESULT_BANG_OK_SRC: &str = r#"f>R n t;v=num! "42";~v"#;
const RESULT_BANG_ERR_SRC: &str = r#"f>R n t;v=num! "abc";~v"#;

fn check_result_ok(engine: &str) {
    let out = run(engine, RESULT_BANG_OK_SRC, "f");
    // Tree prints `~42`, VM prints `~~42` (pre-existing wrap-display
    // discrepancy unrelated to this change). Both contain "42" and start with
    // a wrap marker.
    assert!(
        out.starts_with('~') && out.contains("42"),
        "engine={engine}: expected wrapped 42, got {out}"
    );
}

fn check_result_err(engine: &str) {
    // Err propagates; the `~v` wrap is bypassed by the early return.
    let out = run(engine, RESULT_BANG_ERR_SRC, "f");
    assert!(
        out.contains("abc"),
        "expected err containing abc, got {out}"
    );
}

#[test]
fn result_bang_ok_tree() {
    check_result_ok("--run-tree");
}

#[test]
fn result_bang_ok_vm() {
    check_result_ok("--run-vm");
}

#[test]
fn result_bang_err_tree() {
    check_result_err("--run-tree");
}

#[test]
fn result_bang_err_vm() {
    check_result_err("--run-vm");
}
