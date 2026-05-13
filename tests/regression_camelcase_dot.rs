// Regression tests for camelCase field access on records.
//
// Real-world JSON from NVD, AWS, Stripe, GitHub is overwhelmingly camelCase
// (`baseSeverity`, `stargazersCount`, `paymentMethod`). The strict identifier
// rule (lowercase + hyphens) made `r.baseSeverity` trip ILO-L003 at the lexer
// because uppercase mid-ident is rejected. The fix mirrors the snake_case
// post-pass: when an uppercase character appears flush against an `Ident` that
// is itself flush against a preceding `Dot`/`DotQuestion`, the lexer absorbs
// the camelCase tail (`[A-Za-z0-9]+`) into a single `Ident` token.
//
// Bindings (`fooBar=5`) still emit ILO-L003.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine).arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(src: &str) -> String {
    let out = ilo()
        .args([src, "--run-tree", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success(), "expected failure for `{src}`");
    String::from_utf8_lossy(&out.stderr).to_string()
}

// `r.baseSeverity` (capital is a type sigil `S`) returns "HIGH" across engines.
const SIGIL: &str = "f j:t>R n t;r=jpar! j;r.baseSeverity";

fn check_sigil(engine: &str) {
    assert_eq!(
        run(engine, SIGIL, "f", &[r#"{"baseSeverity":"HIGH"}"#]),
        "HIGH",
        "engine={engine}"
    );
}

#[test]
fn camel_field_sigil_tree() {
    check_sigil("--run-tree");
}

#[test]
fn camel_field_sigil_vm() {
    check_sigil("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn camel_field_sigil_cranelift() {
    check_sigil("--run-cranelift");
}

// `r.gitURL` (capital is a non-sigil `U`) — exercises the second lex path.
const NON_SIGIL: &str = "f j:t>R n t;r=jpar! j;r.gitURL";

fn check_non_sigil(engine: &str) {
    assert_eq!(
        run(engine, NON_SIGIL, "f", &[r#"{"gitURL":"x"}"#]),
        "x",
        "engine={engine}"
    );
}

#[test]
fn camel_field_non_sigil_tree() {
    check_non_sigil("--run-tree");
}

#[test]
fn camel_field_non_sigil_vm() {
    check_non_sigil("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn camel_field_non_sigil_cranelift() {
    check_non_sigil("--run-cranelift");
}

// Chained camelCase access: `r.baseSeverity.label`.
const CHAINED: &str = "f j:t>R n t;r=jpar! j;r.baseSeverity.label";

fn check_chained(engine: &str) {
    assert_eq!(
        run(engine, CHAINED, "f", &[r#"{"baseSeverity":{"label":"x"}}"#]),
        "x",
        "engine={engine}"
    );
}

#[test]
fn camel_field_chained_tree() {
    check_chained("--run-tree");
}

#[test]
fn camel_field_chained_vm() {
    check_chained("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn camel_field_chained_cranelift() {
    check_chained("--run-cranelift");
}

// camelCase + trailing digit: `r.field2Name`.
const DIGIT: &str = "f j:t>R n t;r=jpar! j;r.field2Name";

fn check_digit(engine: &str) {
    assert_eq!(
        run(engine, DIGIT, "f", &[r#"{"field2Name":42}"#]),
        "42",
        "engine={engine}"
    );
}

#[test]
fn camel_field_digit_tree() {
    check_digit("--run-tree");
}

#[test]
fn camel_field_digit_vm() {
    check_digit("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn camel_field_digit_cranelift() {
    check_digit("--run-cranelift");
}

// Safe access on a camelCase field: `r.?baseSeverity`.
#[test]
fn camel_field_safe_access_tree() {
    let out = run(
        "--run-tree",
        "f j:t>R n t;r=jpar! j;r.?baseSeverity",
        "f",
        &[r#"{"baseSeverity":"HIGH"}"#],
    );
    assert_eq!(out, "HIGH");
}

// Mixed camelCase + snake_case: `r.gitURL_count`. The camelCase pass runs
// first inside the main lex loop, producing `Ident("gitURL")`; then the
// post-lex snake_case pass stitches `_count` onto the end.
const MIXED: &str = "f j:t>R n t;r=jpar! j;r.gitURL_count";

fn check_mixed(engine: &str) {
    assert_eq!(
        run(engine, MIXED, "f", &[r#"{"gitURL_count":7}"#]),
        "7",
        "engine={engine}"
    );
}

#[test]
fn camel_field_mixed_snake_tree() {
    check_mixed("--run-tree");
}

#[test]
fn camel_field_mixed_snake_vm() {
    check_mixed("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn camel_field_mixed_snake_cranelift() {
    check_mixed("--run-cranelift");
}

// ---- Negative regressions: strict lowercase rule preserved for bindings ----

#[test]
fn camel_binding_still_errors_sigil() {
    // `fooSet=5` (S is a type sigil) in a binding position must still emit
    // ILO-L003 with the lowercase-suggestion friendly message.
    let err = run_err("f>n;fooSet=5;fooSet");
    assert!(err.contains("ILO-L003"), "stderr: {err}");
    assert!(err.contains("fooSet"), "stderr: {err}");
}

#[test]
fn camel_binding_still_errors_non_sigil() {
    // `fooBar=5` (B is not a sigil) — same expectation.
    let err = run_err("f>n;fooBar=5;fooBar");
    assert!(err.contains("ILO-L003"), "stderr: {err}");
    assert!(err.contains("fooBar"), "stderr: {err}");
}

#[test]
fn dot_then_plain_ident_unchanged() {
    // `r.foo` (no uppercase) must still parse as a plain field access.
    let out = run(
        "--run-tree",
        "f j:t>R n t;r=jpar! j;r.foo",
        "f",
        &[r#"{"foo":3}"#],
    );
    assert_eq!(out, "3");
}

#[test]
fn dot_then_camel_space_ident_keeps_tokens_separate() {
    // `r.fooBar baz` must absorb `fooBar` as the field name but leave `baz` as
    // a separate token. `baz` is unbound so the program should fail rather
    // than silently treat it as part of the field name.
    let err = run_err("f j:t>R n t;r=jpar! j;r.fooBar baz");
    assert!(!err.is_empty(), "expected an error, got empty stderr");
    // The error should NOT be the L003 uppercase-rejection — the camelCase
    // tail merged correctly. It should be a downstream parse/resolve error.
    assert!(!err.contains("baseSeverity"), "stderr: {err}");
}
