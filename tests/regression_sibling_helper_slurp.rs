// Regression: a function ending in a bare call followed by a sibling function
// declaration must not slurp the next function's name as an argument.
//
// Previously, the doc rule was: "non-last function must not end in a bare call;
// wrap the last expression in (...)". The fix detects, at the top-level body
// boundary, when the next tokens form a function-declaration header and
// terminates the current body cleanly.
//
// Discriminator: a real fn decl has `>` before its body's first `;`. Record
// constructions (`Outer a:1 b:2`) and other `Ident Ident :` shapes do not,
// so they continue to parse as statements.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str) -> (bool, String, String) {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
    )
}

// The doc's failing shape: three functions, the first ends in a bare call.
// Before the fix, `cntval` was slurped as a third argument to `has`.
const DOC_REPRO: &str = "isn nm:t>b;has nm \"and \";cntval s:t>n;5;main>n;cntval \"hello\"";

fn check_doc_repro(engine: &str) {
    let (ok, stdout, stderr) = run(engine, DOC_REPRO, "main");
    assert!(ok, "engine={engine}: expected success, stderr={stderr}");
    assert_eq!(stdout, "5", "engine={engine}");
}

#[test]
fn doc_repro_tree() {
    check_doc_repro("--run-tree");
}

#[test]
fn doc_repro_vm() {
    check_doc_repro("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn doc_repro_cranelift() {
    check_doc_repro("--run-cranelift");
}

// A function ending in a bare call followed by another function declaration
// must work without parenthesising the trailing expression.
const BARE_CALL_THEN_SIBLING: &str = "dbl x:n>n;*x 2;f>n;dbl 21";

fn check_bare_call_then_sibling(engine: &str) {
    let (ok, stdout, stderr) = run(engine, BARE_CALL_THEN_SIBLING, "f");
    assert!(ok, "engine={engine}: stderr={stderr}");
    assert_eq!(stdout, "42", "engine={engine}");
}

#[test]
fn bare_call_then_sibling_tree() {
    check_bare_call_then_sibling("--run-tree");
}

#[test]
fn bare_call_then_sibling_vm() {
    check_bare_call_then_sibling("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn bare_call_then_sibling_cranelift() {
    check_bare_call_then_sibling("--run-cranelift");
}

// Record construction inside a statement following a `;` must still parse as
// a record, not be mistaken for a fn-decl header. Discriminator: a real fn
// decl always has `>` before the body's first `;`; this record does not.
const RECORD_AFTER_SEMI: &str = "type pt{x:n;y:n} f>n;p=pt x:3 y:4;+p.x p.y";

fn check_record_after_semi(engine: &str) {
    let (ok, stdout, stderr) = run(engine, RECORD_AFTER_SEMI, "f");
    assert!(ok, "engine={engine}: stderr={stderr}");
    assert_eq!(stdout, "7", "engine={engine}");
}

#[test]
fn record_after_semi_tree() {
    check_record_after_semi("--run-tree");
}

#[test]
fn record_after_semi_vm() {
    check_record_after_semi("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn record_after_semi_cranelift() {
    check_record_after_semi("--run-cranelift");
}

// The parenthesised workaround must continue to work (no regression).
const PAREN_WORKAROUND: &str =
    "isn nm:t>b;(has nm \"and \");cntval s:t>n;5;main>n;cntval \"hello\"";

fn check_paren_workaround(engine: &str) {
    let (ok, stdout, stderr) = run(engine, PAREN_WORKAROUND, "main");
    assert!(ok, "engine={engine}: stderr={stderr}");
    assert_eq!(stdout, "5", "engine={engine}");
}

#[test]
fn paren_workaround_tree() {
    check_paren_workaround("--run-tree");
}

#[test]
fn paren_workaround_vm() {
    check_paren_workaround("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn paren_workaround_cranelift() {
    check_paren_workaround("--run-cranelift");
}
