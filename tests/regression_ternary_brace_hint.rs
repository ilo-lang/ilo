// Regression tests: context-aware ILO-P009 hints when a `{` appears mid
// prefix-ternary operand parse. Two shapes covered:
//
//   1. `?<subj> <oper>{<lit>:body; _:fallback}` — Rust-style match-on-value
//      reached for the wrong shape. Hint should suggest the parenthesised
//      `?(<expr>){...}` form (or single-token `?<subj>{...}`).
//   2. `?h cond{body}` — three-form conditional confusion (`?h cond a b` vs
//      `cond{a}{b}` vs `cond{body}`). Hint should enumerate all three.
//
// Both shapes are pure parser diagnostics — exercised via `--parse` so all
// engines (tree / VM / Cranelift) share the same front-end behaviour.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn stderr_of(args: &[&str]) -> String {
    let out = ilo().args(args).output().expect("failed to run ilo");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

// ---------------------------------------------------------------------------
// Hint #1: match-on-value shape mis-keyed with leading `?h`
// ---------------------------------------------------------------------------

#[test]
fn ternary_match_arm_shape_has_hint() {
    // `?h x{0:1; _:2}` — agent wanted `?x{0:1; _:2}` (or `?(x){0:1; _:2}`).
    // After parsing subject `h` and first operand `x`, the parser hits `{`
    // and would otherwise emit a bare ILO-P009 with no hint.
    let src = "main x:n>n;?h x{0:1;_:2}";
    let stderr = stderr_of(&[src, "--vm", "main", "1"]);
    assert!(
        stderr.contains("ILO-P009"),
        "expected ILO-P009, got: {stderr}"
    );
    assert!(
        stderr.contains("match-on-value arms"),
        "expected match-on-value framing in message, got: {stderr}"
    );
    assert!(
        stderr.contains("?(") || stderr.contains("?x{"),
        "expected hint pointing at parenthesised or single-token match form, got: {stderr}"
    );
}

#[test]
fn ternary_match_arm_shape_uses_first_operand_in_hint() {
    // The recommended single-token form should echo the operand the agent
    // most likely wanted as the match subject (here: `mn`).
    let src = "main x:n>n;mn=x;?h mn{0:1;_:2}";
    let stderr = stderr_of(&[src, "--vm", "main", "1"]);
    assert!(
        stderr.contains("ILO-P009"),
        "expected ILO-P009, got: {stderr}"
    );
    assert!(
        stderr.contains("?mn{"),
        "hint should suggest `?mn{{...}}` using the parsed operand, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Hint #2: three-form conditional confusion
// ---------------------------------------------------------------------------

#[test]
fn ternary_h_cond_brace_body_has_hint() {
    // `?h ok{1}` — agent wanted one of the three canonical shapes. The
    // brace body is a single expression (NOT a `<lit>:` arm), so the hint
    // should enumerate all three forms rather than the match-on-value
    // recommendation.
    let src = "main x:n>n;ok=>x 0;?h ok{1}";
    let stderr = stderr_of(&[src, "--vm", "main", "1"]);
    assert!(
        stderr.contains("ILO-P009"),
        "expected ILO-P009, got: {stderr}"
    );
    // Should NOT misfire as match-arm shape (single non-literal-arm body)
    assert!(
        !stderr.contains("match-on-value arms"),
        "should not flag as match-on-value when body is a bare expression, got: {stderr}"
    );
    // Should cover all three canonical conditional shapes
    assert!(
        stderr.contains("?h ok a b"),
        "hint should mention prefix-ternary `?h cond a b` form with the parsed condition, got: {stderr}"
    );
    assert!(
        stderr.contains("ok{a}{b}"),
        "hint should mention brace-ternary `cond{{a}}{{b}}` form, got: {stderr}"
    );
    assert!(
        stderr.contains("ok{body}"),
        "hint should mention braced-conditional `cond{{body}}` form, got: {stderr}"
    );
}

#[test]
fn ternary_h_cond_brace_body_hint_in_expr_position() {
    // Same shape but in expr position (RHS of binding). The hint must fire
    // in `parse_prefix_ternary` (expression path) too, not only the
    // statement-position `parse_match_stmt`.
    let src = "main x:n>n;ok=>x 0;r=?h ok{1};r";
    let stderr = stderr_of(&[src, "--vm", "main", "1"]);
    assert!(
        stderr.contains("ILO-P009"),
        "expected ILO-P009 in expr position, got: {stderr}"
    );
    assert!(
        stderr.contains("three conditional shapes")
            || (stderr.contains("?h ok a b") && stderr.contains("ok{body}")),
        "expected three-shape hint in expr position, got: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// Negative: shapes that already work must still parse cleanly
// ---------------------------------------------------------------------------

#[test]
fn parenthesised_match_subject_still_parses() {
    // Canonical `?(<expr>){...}` form must not be flagged by the new hint.
    let src = "main x:n>n;?(mod x 3){0:10;1:20;_:0}";
    let out = ilo()
        .args([src, "--vm", "main", "1"])
        .output()
        .expect("run ilo");
    assert!(
        out.status.success(),
        "parenthesised match subject should parse, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn prefix_ternary_three_operand_still_parses() {
    // `?h cond a b` keyword form must not be flagged when no `{` follows.
    let src = "main x:n>n;ok=>x 0;?h ok 1 0";
    let out = ilo()
        .args([src, "--vm", "main", "1"])
        .output()
        .expect("run ilo");
    assert!(
        out.status.success(),
        "?h cond a b should parse, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
