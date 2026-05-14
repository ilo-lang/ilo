// Regression tests for nested generic types via parentheses.
//
// Previously type expressions like `R (L n) t` failed with
// `ILO-P007 expected type, got LParen`: parse_type only accepted
// single-token atom types (n, t, b, _, RecordName) as arguments to
// type ctors (R, L, O, M, F, S). Wrapping a compound type in parens
// is now supported in any type-expression position, so signatures
// can precisely describe Results-of-lists, lists-of-lists, etc.
// Type signatures are verifier-only, so all engines exercise this
// the same way (parse + verify happens once before lowering).

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

// Result-of-list-of-numbers (the doc's motivating example).
const RESULT_OF_LIST: &str = "f>R (L n) t;~[1,2,3]";
// List-of-lists.
const LIST_OF_LIST: &str = "g>L (L n);[[1,2],[3,4]]";
// Optional-of-result — verifier-only, returning nil satisfies any O.
const OPT_OF_RESULT: &str = "h>O (R n t);nil";
// Triple-nested: Result of (List of (Result of n,t)) error t.
const TRIPLE_NESTED: &str = "f>R (L (R n t)) t;~[~1,~2]";
// Single-token in parens should be transparent: `R (n) t` == `R n t`.
const PARENS_AROUND_ATOM: &str = "f>R (n) t;~1";
// Pre-existing flat form keeps working (no regression).
const FLAT_RESULT: &str = "f>R n t;~1";

fn check_all(engine: &str) {
    // Top-level Value::Ok prints bare on stdout (the leading `~` is stripped
    // by the symmetric stdout/stderr split — see
    // regression_main_ok_stdout_bare.rs). Nested `Value::Ok` inside a list
    // keeps its `~` because Display formatting is unchanged for non-top-level
    // contexts.
    assert_eq!(
        run(engine, RESULT_OF_LIST, "f"),
        "[1, 2, 3]",
        "R (L n) t engine={engine}"
    );
    assert_eq!(
        run(engine, LIST_OF_LIST, "g"),
        "[[1, 2], [3, 4]]",
        "L (L n) engine={engine}"
    );
    assert_eq!(
        run(engine, OPT_OF_RESULT, "h"),
        "nil",
        "O (R n t) engine={engine}"
    );
    assert_eq!(
        run(engine, TRIPLE_NESTED, "f"),
        "[~1, ~2]",
        "R (L (R n t)) t engine={engine} — outer `~` stripped, inner `~`s kept"
    );
    assert_eq!(
        run(engine, PARENS_AROUND_ATOM, "f"),
        "1",
        "R (n) t engine={engine}"
    );
    assert_eq!(
        run(engine, FLAT_RESULT, "f"),
        "1",
        "R n t (flat, no regression) engine={engine}"
    );
}

#[test]
fn nested_generic_types_tree() {
    check_all("--run-tree");
}

#[test]
fn nested_generic_types_vm() {
    check_all("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn nested_generic_types_cranelift() {
    check_all("--run-cranelift");
}
