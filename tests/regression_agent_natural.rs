// Regression tests for the agent-natural surface (compat/agent-natural).
//
// Spec: SPEC-AGENT-NATURAL.md. Each new surface form is a parse-time desugar
// onto an existing AST node, so the verifier and every backend see no new
// shapes. These tests pin that the new forms run identically on the VM and
// Cranelift JIT backends — if any backend ever sees something it shouldn't,
// the abstraction has leaked and one of the engines will diverge.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str, arg: &str) -> String {
    let out = ilo()
        .args([src, engine, entry, arg])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// ── match arm block bodies ──────────────────────────────────────────────────
//
// Multi-statement match arm bodies via `pat:{stmt;stmt;expr}` already live in
// `parse_arm_body`. These tests pin the behaviour cross-engine so a future
// refactor can't silently drop them (spec §2.3 calls match-arm blocks out as
// a v0 item — confirming it works on both backends is the contract).

const MATCH_BLOCK_OK_ARM: &str = "go n:n>n;r=?n{0:^\"zero\";_:~n};?r{~v:{d=*v 2;+d 1};^_:0}\n";

#[test]
fn match_arm_block_ok_vm() {
    assert_eq!(run("--vm", MATCH_BLOCK_OK_ARM, "go", "10"), "21");
}

#[test]
#[cfg(feature = "cranelift")]
fn match_arm_block_ok_jit() {
    assert_eq!(run("--jit", MATCH_BLOCK_OK_ARM, "go", "10"), "21");
}

const MATCH_BLOCK_ERR_ARM: &str =
    "go n:n>t;r=?n{0:^\"oops\";_:~\"k\"};?r{~v:str v;^er:{tag=\"err: \";+tag er}}\n";

#[test]
fn match_arm_block_err_vm() {
    assert_eq!(run("--vm", MATCH_BLOCK_ERR_ARM, "go", "0"), "err: oops");
}

#[test]
#[cfg(feature = "cranelift")]
fn match_arm_block_err_jit() {
    assert_eq!(run("--jit", MATCH_BLOCK_ERR_ARM, "go", "0"), "err: oops");
}

// ── if / else ───────────────────────────────────────────────────────────────
//
// `if cond { a } else { b }` at expression position desugars to `Expr::Ternary`
// (the AST that `cond{a}{b}` already produces). `if cond { body }` at statement
// position desugars to `Stmt::Guard` with optional `else_body`. Spec §2.2.

const IF_ELSE_VALUE: &str = "myabs n:n>n;if >=n 0 { n } else { -0 n }\n";

#[test]
fn if_else_value_vm() {
    assert_eq!(run("--vm", IF_ELSE_VALUE, "myabs", "-7"), "7");
    assert_eq!(run("--vm", IF_ELSE_VALUE, "myabs", "5"), "5");
}

#[test]
#[cfg(feature = "cranelift")]
fn if_else_value_jit() {
    assert_eq!(run("--jit", IF_ELSE_VALUE, "myabs", "-7"), "7");
    assert_eq!(run("--jit", IF_ELSE_VALUE, "myabs", "5"), "5");
}

const IF_STMT_ELSE: &str = "label n:n>t;t=\"\";if >=n 0 { t=\"pos\" } else { t=\"neg\" };t\n";

#[test]
fn if_stmt_else_vm() {
    assert_eq!(run("--vm", IF_STMT_ELSE, "label", "3"), "pos");
    assert_eq!(run("--vm", IF_STMT_ELSE, "label", "-3"), "neg");
}

#[test]
#[cfg(feature = "cranelift")]
fn if_stmt_else_jit() {
    assert_eq!(run("--jit", IF_STMT_ELSE, "label", "3"), "pos");
    assert_eq!(run("--jit", IF_STMT_ELSE, "label", "-3"), "neg");
}

// `if cond { body }` without `else` — spec §7 open question: returns nil at
// expression position. Here we exercise the statement form where the guard
// runs (or not) and the enclosing fn's last expr is the return value.
const IF_STMT_NO_ELSE: &str = "guard-pos n:n>t;t=\"start\";if >=n 0 { t=\"pos\" };t\n";

#[test]
fn if_stmt_no_else_vm() {
    assert_eq!(run("--vm", IF_STMT_NO_ELSE, "guard-pos", "1"), "pos");
    assert_eq!(run("--vm", IF_STMT_NO_ELSE, "guard-pos", "-1"), "start");
}

#[test]
#[cfg(feature = "cranelift")]
fn if_stmt_no_else_jit() {
    assert_eq!(run("--jit", IF_STMT_NO_ELSE, "guard-pos", "1"), "pos");
    assert_eq!(run("--jit", IF_STMT_NO_ELSE, "guard-pos", "-1"), "start");
}

// Parity: agent-natural `if/else` vs the existing brace-ternary form.
const PARITY_IF_VS_BRACE_NATURAL: &str = "myabs n:n>n;if >=n 0 { n } else { -0 n }\n";
const PARITY_IF_VS_BRACE_LEGACY: &str = "myabs n:n>n;v=>=n 0{n}{-0 n};v\n";

#[test]
fn if_else_matches_brace_ternary_vm() {
    let a = run("--vm", PARITY_IF_VS_BRACE_NATURAL, "myabs", "-9");
    let b = run("--vm", PARITY_IF_VS_BRACE_LEGACY, "myabs", "-9");
    assert_eq!(a, b);
    assert_eq!(a, "9");
}

#[test]
#[cfg(feature = "cranelift")]
fn if_else_matches_brace_ternary_jit() {
    let a = run("--jit", PARITY_IF_VS_BRACE_NATURAL, "myabs", "-9");
    let b = run("--jit", PARITY_IF_VS_BRACE_LEGACY, "myabs", "-9");
    assert_eq!(a, b);
    assert_eq!(a, "9");
}

// ── while ───────────────────────────────────────────────────────────────────
//
// `while cond { body }` desugars to `Stmt::While` — same AST as `wh cond{body}`.
// Spec §2.4.

const WHILE_LOOP: &str = "fac n:n>n;a=1;i=1;while <=i n{a=*a i;i=+i 1};a\n";

#[test]
fn while_loop_vm() {
    assert_eq!(run("--vm", WHILE_LOOP, "fac", "5"), "120");
}

#[test]
#[cfg(feature = "cranelift")]
fn while_loop_jit() {
    assert_eq!(run("--jit", WHILE_LOOP, "fac", "5"), "120");
}

const WHILE_LEGACY: &str = "fac n:n>n;a=1;i=1;wh <=i n{a=*a i;i=+i 1};a\n";

#[test]
fn while_matches_wh_vm() {
    assert_eq!(
        run("--vm", WHILE_LOOP, "fac", "6"),
        run("--vm", WHILE_LEGACY, "fac", "6"),
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn while_matches_wh_jit() {
    assert_eq!(
        run("--jit", WHILE_LOOP, "fac", "6"),
        run("--jit", WHILE_LEGACY, "fac", "6"),
    );
}

// ── for ─────────────────────────────────────────────────────────────────────
//
// `for x in xs { body }` and `for i in a..b { body }` desugar to
// `Stmt::ForEach` / `Stmt::ForRange` — same AST as `@x xs{body}` / `@i a..b{body}`.
// Spec §2.4.

const FOR_RANGE: &str = "sum-to n:n>n;t=0;for i in 1..+n 1{t=+t i};t\n";

#[test]
fn for_range_vm() {
    assert_eq!(run("--vm", FOR_RANGE, "sum-to", "10"), "55");
}

#[test]
#[cfg(feature = "cranelift")]
fn for_range_jit() {
    assert_eq!(run("--jit", FOR_RANGE, "sum-to", "10"), "55");
}

const FOR_EACH: &str = "cat-words s:t>t;ws=spl s \",\";out=\"\";for w in ws{out=+out w};out\n";

#[test]
fn for_each_vm() {
    assert_eq!(run("--vm", FOR_EACH, "cat-words", "a,b,c"), "abc");
}

#[test]
#[cfg(feature = "cranelift")]
fn for_each_jit() {
    assert_eq!(run("--jit", FOR_EACH, "cat-words", "a,b,c"), "abc");
}

const FOR_RANGE_LEGACY: &str = "sum-to n:n>n;t=0;@i 1..+n 1{t=+t i};t\n";

#[test]
fn for_range_matches_at_vm() {
    assert_eq!(
        run("--vm", FOR_RANGE, "sum-to", "20"),
        run("--vm", FOR_RANGE_LEGACY, "sum-to", "20"),
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn for_range_matches_at_jit() {
    assert_eq!(
        run("--jit", FOR_RANGE, "sum-to", "20"),
        run("--jit", FOR_RANGE_LEGACY, "sum-to", "20"),
    );
}

// ── identifier-collision guard ──────────────────────────────────────────────
//
// Hyphenated identifiers prefixed with `for`/`while`/`else`/`in` (`for-each`,
// `in-window`, `else-clause`) must keep parsing as `Ident`, not as keyword
// followed by garbage. Logos picks the longest match, so the ident regex
// `[a-z][a-z0-9]*(-[a-z0-9]+)*` wins over the bare keyword token.

#[test]
fn hyphen_ident_with_keyword_prefix_vm() {
    let src = "for-each xs:Lt>t;cat xs \"|\"\ngo s:t>t;ws=spl s \",\";for-each ws\n";
    assert_eq!(run("--vm", src, "go", "a,b,c"), "a|b|c");
}
