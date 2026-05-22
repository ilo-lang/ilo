// Regression tests for multiple-bound conjunctions on generic type variables
// (ILO-386).
//
// Syntax: `<a:numeric+comparable>` — the type variable `a` must satisfy BOTH
// `numeric` AND `comparable`. This is an intersection of bounds, not a union.
//
// This extends ILO-61 (single-bound generics) and ILO-385 (generic return
// types).

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Run inline snippet with entry-point, assert success, return stdout.
fn run_ok(src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, entry])
        .output()
        .expect("failed to spawn ilo");
    assert!(
        out.status.success(),
        "expected success for snippet {src:?} entry={entry}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Run inline snippet and assert failure with specific error code in stderr.
fn assert_error(src: &str, expected_code: &str) {
    // First ident before `<` or space is the entry function name.
    let entry = src
        .find(|c: char| !c.is_ascii_alphabetic() && c != '-')
        .map(|i| &src[..i])
        .unwrap_or("f");
    // Collect entry from the `main` function if present.
    let entry = if src.contains("main>") { "main" } else { entry };
    let out = ilo()
        .args([src, entry])
        .output()
        .expect("failed to spawn ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "expected failure for snippet {src:?} (code {expected_code}), but succeeded"
    );
    assert!(
        stderr.contains(expected_code),
        "expected {expected_code} in stderr for snippet {src:?}\nstderr was:\n{stderr}"
    );
}

// ---- Acceptance: numeric+comparable accepts `n` (satisfies both) ----

#[test]
fn multibound_call_with_number() {
    // `n` satisfies both `numeric` and `comparable`.
    let src = "double<a:numeric+comparable> x:a>a;+ x x\nmain>n;double 7";
    assert_eq!(run_ok(src, "main"), "14");
}

#[test]
fn multibound_identity_numeric_comparable() {
    let src = "f<a:numeric+comparable> x:a>a;x\nmain>n;f 42";
    assert_eq!(run_ok(src, "main"), "42");
}

// ---- Rejection: numeric+comparable rejects `t` (fails numeric) ----

#[test]
fn multibound_numeric_comparable_rejects_t() {
    // `t` satisfies `comparable` but NOT `numeric` — should fail ILO-T044.
    let src = "f<a:numeric+comparable> x:a>a;x\nmain>t;f \"hello\"";
    assert_error(src, "ILO-T044");
}

// ---- Rejection: numeric+text rejects `b` (fails both numeric and text) ----

#[test]
fn multibound_numeric_text_rejects_b() {
    // `b` (bool) satisfies neither numeric nor text.
    let src = "f<a:numeric+text> x:a>a;x\nmain>b;f true";
    assert_error(src, "ILO-T044");
}

// ---- Three-bound conjunction: numeric+comparable+text (impossible to satisfy) ----

#[test]
fn multibound_three_way_impossible_with_n() {
    // `n` satisfies numeric and comparable but NOT text.
    let src = "f<a:numeric+comparable+text> x:a>a;x\nmain>n;f 1";
    assert_error(src, "ILO-T044");
}

#[test]
fn multibound_three_way_impossible_with_t() {
    // `t` satisfies comparable and text but NOT numeric.
    let src = "f<a:numeric+comparable+text> x:a>a;x\nmain>t;f \"hi\"";
    assert_error(src, "ILO-T044");
}

// ---- Parsing: single bound still works (no regression) ----

#[test]
fn single_bound_no_regression() {
    let src = "inc<a:numeric> x:a>a;+ x 1\nmain>n;inc 5";
    assert_eq!(run_ok(src, "main"), "6");
}

// ---- Parsing: unbounded type variable still works ----

#[test]
fn unbound_type_var_no_regression() {
    let src = "ident<a> x:a>a;x\nmain>n;ident 99";
    assert_eq!(run_ok(src, "main"), "99");
}

// ---- Comparable+numeric = same as numeric+comparable (order should not matter) ----

#[test]
fn multibound_order_comparable_then_numeric() {
    let src = "f<a:comparable+numeric> x:a>a;x\nmain>n;f 5";
    assert_eq!(run_ok(src, "main"), "5");
}

#[test]
fn multibound_order_comparable_then_numeric_rejects_t() {
    let src = "f<a:comparable+numeric> x:a>a;x\nmain>t;f \"x\"";
    assert_error(src, "ILO-T044");
}
