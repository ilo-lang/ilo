// Regression tests for match arm brace-block bodies:
//
//   ?expr{~v:{stmt;stmt;final-expr} ^e:body}
//
// Before the fix, the parser rejected `{` immediately after the arm `:` with
// `ILO-P009: expected expression, got LBrace`. Personas paid a helper-function
// tax on every Result-handling site because the only way to put multi-step
// logic inside an arm was to factor it out into a separate function. The
// `;`-inline form (`~v:stmt1;stmt2;final-expr`) was already accepted by the
// parser but is visually noisy and ambiguous to a tired model when the body
// contains call-shapes that resemble patterns. Brace-block form mirrors the
// existing `=cond{block}` grammar and makes the arm boundary unambiguous.
//
// These tests pin cross-engine behaviour across tree / VM / Cranelift so the
// shape can't silently regress.

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
        "ilo {engine} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm"];

// Single-expression arms (existing behaviour) must keep working unchanged.
const SINGLE_EXPR: &str = r#"f>n;?2{0:0;1:1;2:42;_:99}"#;

#[test]
fn single_expr_arm_preserved_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, SINGLE_EXPR, "f");
        assert_eq!(out.trim(), "42", "{engine}: {out:?}");
    }
}

// Brace-block arm with a local binding and a final expression.
const BLOCK_WITH_LOCAL: &str = r#"f>n;r=num "10";?r{~v:{d=*v 2;+d 1};^e:0}"#;

#[test]
fn brace_block_with_local_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, BLOCK_WITH_LOCAL, "f");
        assert_eq!(out.trim(), "21", "{engine}: {out:?}");
    }
}

// Brace-block on the Err arm too.
const BLOCK_ON_ERR: &str = r#"f>t;r=num "oops";?r{~v:str v;^e:{tag="err: ";+tag e}}"#;

#[test]
fn brace_block_on_err_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, BLOCK_ON_ERR, "f");
        assert_eq!(out.trim(), "err: oops", "{engine}: {out:?}");
    }
}

// Nested match: brace-block body contains another match expression.
const NESTED_MATCH: &str = r#"f>t;r=num "0";?r{~v:{?v{0:"zero";_:"nonzero"}};^e:"bad"}"#;

#[test]
fn nested_match_in_block_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, NESTED_MATCH, "f");
        assert_eq!(out.trim(), "zero", "{engine}: {out:?}");
    }
}

// Multiple brace-block arms on a bool match: each branch does setup work
// before producing its value.
const BOOL_BLOCKS: &str =
    r#"f>t;?true{true:{tag="OK: ";+tag "all good"};false:{tag="FAIL: ";+tag "see logs"}}"#;

#[test]
fn bool_match_brace_blocks_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, BOOL_BLOCKS, "f");
        assert_eq!(out.trim(), "OK: all good", "{engine}: {out:?}");
    }
}

// Existing inline `;`-separated arm body must keep working — the brace-block
// path is purely additive.
const INLINE_SEMI: &str = r#"f>n;r=num "10";?r{~v:d=*v 2;+d 1;^e:0}"#;

#[test]
fn inline_semi_arm_body_preserved_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, INLINE_SEMI, "f");
        assert_eq!(out.trim(), "21", "{engine}: {out:?}");
    }
}

// Match-as-expression in RHS of a binding, with a brace-block arm body.
const MATCH_EXPR_BLOCK: &str = r#"f>n;r=num "5";x=?r{~v:{d=*v 3;+d 1};^e:0};+x 0"#;

#[test]
fn match_expr_brace_block_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, MATCH_EXPR_BLOCK, "f");
        assert_eq!(out.trim(), "16", "{engine}: {out:?}");
    }
}
