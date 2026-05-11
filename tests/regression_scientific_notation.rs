// Regression tests for scientific notation in number literals.
//
// Real-world numerics (finance, science, gov budgets) routinely uses
// scientific notation like `1e9`, `2.5e-3`, `1.5E10`. The lexer accepts
// an optional `[eE][+-]?[0-9]+` suffix on the existing number regex, and
// `f64::from_str` handles the parse natively.
//
// We exercise every engine to ensure the value flows through the lexer,
// parser, and each runtime identically.

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

fn run_fail(engine: &str, src: &str, entry: &str) -> String {
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

// `1e9` → 1_000_000_000
const E9_SRC: &str = "f>n;1e9";

fn check_e9(engine: &str) {
    assert_eq!(run(engine, E9_SRC, "f"), "1000000000", "engine={engine}");
}

#[test]
fn sci_e9_tree() {
    check_e9("--run-tree");
}

#[test]
fn sci_e9_vm() {
    check_e9("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn sci_e9_cranelift() {
    check_e9("--run-cranelift");
}

// `2.5e-3` → 0.0025 (negative exponent; the `-` is part of the number,
// not a binary operator).
const NEG_EXP_SRC: &str = "f>n;2.5e-3";

fn check_neg_exp(engine: &str) {
    assert_eq!(run(engine, NEG_EXP_SRC, "f"), "0.0025", "engine={engine}");
}

#[test]
fn sci_neg_exp_tree() {
    check_neg_exp("--run-tree");
}

#[test]
fn sci_neg_exp_vm() {
    check_neg_exp("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn sci_neg_exp_cranelift() {
    check_neg_exp("--run-cranelift");
}

// Capital `E` works the same as lowercase.
const CAP_E_SRC: &str = "f>n;1.5E10";

fn check_cap_e(engine: &str) {
    assert_eq!(
        run(engine, CAP_E_SRC, "f"),
        "15000000000",
        "engine={engine}"
    );
}

#[test]
fn sci_cap_e_tree() {
    check_cap_e("--run-tree");
}

#[test]
fn sci_cap_e_vm() {
    check_cap_e("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn sci_cap_e_cranelift() {
    check_cap_e("--run-cranelift");
}

// Explicit `+` in exponent.
const PLUS_EXP_SRC: &str = "f>n;1e+5";

fn check_plus_exp(engine: &str) {
    assert_eq!(run(engine, PLUS_EXP_SRC, "f"), "100000", "engine={engine}");
}

#[test]
fn sci_plus_exp_tree() {
    check_plus_exp("--run-tree");
}

#[test]
fn sci_plus_exp_vm() {
    check_plus_exp("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn sci_plus_exp_cranelift() {
    check_plus_exp("--run-cranelift");
}

// `e9` on its own (no leading digit) must still tokenise as an identifier.
// If the lexer wrongly tried to read it as a number, the binding below
// would fail to parse.
const IDENT_E9_SRC: &str = "f>n;e9=7;e9";

#[test]
fn sci_e9_still_identifier_tree() {
    assert_eq!(run("--run-tree", IDENT_E9_SRC, "f"), "7");
}

#[test]
fn sci_e9_still_identifier_vm() {
    assert_eq!(run("--run-vm", IDENT_E9_SRC, "f"), "7");
}

// `1e` alone (incomplete exponent) must not silently succeed. The exponent
// group requires at least one digit, so the lexer falls back to `1` (number)
// followed by `e` (identifier), which the parser rejects.
const INCOMPLETE_SRC: &str = "f>n;1e";

#[test]
fn sci_incomplete_exponent_errors_tree() {
    let stderr = run_fail("--run-tree", INCOMPLETE_SRC, "f");
    assert!(
        !stderr.is_empty(),
        "expected non-empty stderr for incomplete exponent"
    );
}

#[test]
fn sci_incomplete_exponent_errors_vm() {
    let stderr = run_fail("--run-vm", INCOMPLETE_SRC, "f");
    assert!(
        !stderr.is_empty(),
        "expected non-empty stderr for incomplete exponent"
    );
}
