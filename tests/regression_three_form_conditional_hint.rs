// Regression: the three syntactically-distinct conditional shapes in
// ilo (braceless guard `cond expr`, braced conditional `cond{body}`,
// and ternary - both brace `cond{a}{b}` and prefix `?h cond a b`) all
// parse cleanly, and the wrong-form `?h cond{body}` mistake fires the
// parser hint that enumerates all three canonical shapes.
//
// Pairs with `regression_ternary_brace_hint.rs` (which pins the hint
// shape) and `examples/three-conditional-forms.ilo` (which pins
// semantics across engines via the example harness). This test exists
// so the hint copy and the canonical shapes stay in lockstep: if
// someone edits the parser hint to drop one of the three forms, this
// test catches it; if someone breaks one of the three forms, the
// per-form parse assertions catch it.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(args: &[&str]) -> (bool, String, String) {
    let out = ilo().args(args).output().expect("failed to run ilo");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// ---------------------------------------------------------------------------
// Each canonical form parses cleanly on the VM engine.
// ---------------------------------------------------------------------------

#[test]
fn braceless_guard_parses() {
    // `cond expr` early-return guard. The cheapest of the three.
    let src = "f x:n>t;>x 0 \"pos\";\"nonpos\"";
    let (ok, stdout, stderr) = run(&[src, "--vm", "f", "5"]);
    assert!(ok, "braceless guard should parse and run, stderr: {stderr}");
    assert_eq!(stdout.trim(), "pos");

    let (ok, stdout, _) = run(&[src, "--vm", "f", "-1"]);
    assert!(ok);
    assert_eq!(stdout.trim(), "nonpos");
}

#[test]
fn braced_conditional_parses() {
    // `cond{body}` no-early-return conditional execution.
    let src = "f x:n>n;n=x;>x 0{n=+n 100};n";
    let (ok, stdout, stderr) = run(&[src, "--vm", "f", "5"]);
    assert!(ok, "braced conditional should parse, stderr: {stderr}");
    assert_eq!(stdout.trim(), "105");

    let (ok, stdout, _) = run(&[src, "--vm", "f", "-3"]);
    assert!(ok);
    assert_eq!(stdout.trim(), "-3");
}

#[test]
fn brace_ternary_parses() {
    // `cond{a}{b}` value ternary, no early return.
    let src = "f x:n>t;>x 0{\"pos\"}{\"nonpos\"}";
    let (ok, stdout, stderr) = run(&[src, "--vm", "f", "5"]);
    assert!(ok, "brace ternary should parse, stderr: {stderr}");
    assert_eq!(stdout.trim(), "pos");
}

#[test]
fn prefix_ternary_keyword_parses() {
    // `?h cond a b` keyword form - three operand atoms after `?h`.
    let src = "f x:n>t;c=>x 0;?h c \"pos\" \"nonpos\"";
    let (ok, stdout, stderr) = run(&[src, "--vm", "f", "5"]);
    assert!(ok, "?h cond a b should parse, stderr: {stderr}");
    assert_eq!(stdout.trim(), "pos");
}

// ---------------------------------------------------------------------------
// The wrong-form `?h cond{body}` fires the hint enumerating all three
// canonical conditional forms (not the match-on-value hint).
// ---------------------------------------------------------------------------

#[test]
fn wrong_form_hint_lists_all_three_canonical_shapes() {
    // `?h ok{1}` is a parse error - `?h` is the prefix-ternary keyword,
    // so braces after the cond are illegal. The hint should enumerate
    // the three canonical shapes so the agent can pick.
    let src = "main x:n>n;ok=>x 0;?h ok{1}";
    let (ok, _, stderr) = run(&[src, "--vm", "main", "1"]);
    assert!(!ok, "wrong form should error");
    assert!(stderr.contains("ILO-P009"), "expected ILO-P009: {stderr}");
    // All three forms named in the hint, parameterised with the parsed
    // cond ident `ok` so the agent can copy-paste.
    assert!(
        stderr.contains("?h ok a b"),
        "hint should name prefix-ternary `?h cond a b`: {stderr}"
    );
    assert!(
        stderr.contains("ok{a}{b}"),
        "hint should name brace-ternary `cond{{a}}{{b}}`: {stderr}"
    );
    assert!(
        stderr.contains("ok{body}"),
        "hint should name braced-conditional `cond{{body}}`: {stderr}"
    );
    // Should NOT misroute to the match-on-value hint - the brace body
    // is a bare expression, not `<lit>:body` arms.
    assert!(
        !stderr.contains("match-on-value arms"),
        "should not flag as match-on-value: {stderr}"
    );
}
