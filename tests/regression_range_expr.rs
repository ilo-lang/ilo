// Cross-engine regression tests for range bounds in `@` loops.
//
// Until this fix, `parse_foreach` used `parse_atom` for both range bounds,
// so `@j +i 2..n` (prefix-binop start) and `@j 0..*n 2` (prefix-binop end)
// were rejected with ILO-P009. Personas had to bind an intermediate
// (`jst=+i 2;@j jst..n`) for every compound bound. Now both sides accept
// any operand: literals, idents, prefix-binop forms (`+a b`, `-a b`,
// `*a b`, `/a b`), and unary forms (`-x` literal negation).
//
// Call-style bounds (`@j 0..len xs`) explicitly still require an
// intermediate binding; the `call_style_bound_still_requires_binding`
// test below anchors that contract so a future change extending range
// bounds to `parse_call_or_atom` is a deliberate decision.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_text(engine: &str, src: &str) -> String {
    let out = ilo()
        .args([src, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn check_all(src: &str, expected: &str) {
    for engine in ["--run-tree", "--run-vm"] {
        let actual = run_text(engine, src);
        assert_eq!(
            actual, expected,
            "engine={engine} src=`{src}`: got `{actual}`, expected `{expected}`"
        );
    }
    #[cfg(feature = "cranelift")]
    {
        let engine = "--run-cranelift";
        let actual = run_text(engine, src);
        assert_eq!(
            actual, expected,
            "engine={engine} src=`{src}`: got `{actual}`, expected `{expected}`"
        );
    }
}

#[test]
fn plus_op_start_bound() {
    // @j +i 2..n collects 5..10 = [5,6,7,8,9]
    check_all(
        "f>L n;i=3;n=10;xs=[];@j +i 2..n{xs=+=xs j};xs",
        "[5, 6, 7, 8, 9]",
    );
}

#[test]
fn star_op_end_bound() {
    // @j 0..*n 2 collects 0..6 = [0,1,2,3,4,5]
    check_all(
        "f>L n;n=3;xs=[];@j 0..*n 2{xs=+=xs j};xs",
        "[0, 1, 2, 3, 4, 5]",
    );
}

#[test]
fn minus_op_both_bounds() {
    // @j -a 1..-b 1 with a=2,b=6 → 1..5 = [1,2,3,4]
    check_all(
        "f>L n;a=2;b=6;xs=[];@j -a 1..-b 1{xs=+=xs j};xs",
        "[1, 2, 3, 4]",
    );
}

#[test]
fn div_op_start_bound() {
    // @j /n 2..n with n=8 → 4..8 = [4,5,6,7]
    check_all("f>L n;n=8;xs=[];@j /n 2..n{xs=+=xs j};xs", "[4, 5, 6, 7]");
}

#[test]
fn nested_range_with_op_bounds() {
    // Outer @i 0..2, inner @j +i 1..3 collects pairs i*10+j.
    // i=0 -> j in 1..3 -> [1,2]; i=1 -> j in 2..3 -> [2]
    // Total: [1, 2, 12]
    check_all(
        "f>L n;xs=[];@i 0..2{@j +i 1..3{xs=+=xs +*i 10 j}};xs",
        "[1, 2, 12]",
    );
}

#[test]
fn plus_bounds_negative_result_empty() {
    // When start >= end the loop body never runs.
    check_all("f>L n;i=5;n=4;xs=[];@j +i 0..n{xs=+=xs j};xs", "[]");
}

#[test]
fn atom_only_bounds_still_work() {
    // Regression: the existing atom-only forms must keep working.
    check_all("f>n;s=0;@j 0..5{s=+s j};+s 0", "10");
}

#[test]
fn ident_to_ident_bounds_still_work() {
    // Two-ident range, the most common existing form.
    check_all("f>n;a=1;b=4;s=0;@j a..b{s=+s j};+s 0", "6");
}

#[test]
fn call_style_bound_still_requires_binding() {
    // Anchor: bare function-call bounds remain unsupported. If this test
    // starts failing because the call form now parses, that's a deliberate
    // extension and the test should be updated (and the assessment-doc
    // entry reopened to consider the call-form follow-up).
    let out = ilo()
        .args([
            "f>n;xs=[1,2,3];s=0;@j 0..len xs{s=+s j};+s 0",
            "--run-tree",
            "f",
        ])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected call-style range bound to fail; stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn call_style_bound_works_with_binding() {
    // The documented workaround: bind first, then range over the binding.
    check_all("f>n;xs=[1,2,3];n=len xs;s=0;@j 0..n{s=+s j};+s 0", "3");
}
