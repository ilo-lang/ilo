// Regression: src/codegen/js.rs must handle the AST variants added by
// ILO-410 (`todo`/`panic`) and ILO-411 (`|` match alternatives). Without
// these arms the lib fails to compile under `-D warnings` (E0004
// non-exhaustive match), which broke main on 2026-05-22 and blocked the
// merge queue until this fix landed.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn emit_js(src: &str) -> String {
    let out = ilo()
        .arg(src)
        .arg("--emit")
        .arg("js")
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf8")
}

#[test]
fn js_emit_handles_todo() {
    let js = emit_js(r#"f x:n>n;todo "wip""#);
    assert!(
        js.contains("throw new Error('TODO: '"),
        "expected todo->throw codegen, got: {js}"
    );
}

#[test]
fn js_emit_handles_panic() {
    let js = emit_js(r#"f x:n>n;panic "fatal""#);
    assert!(
        js.contains("throw new Error('PANIC: '"),
        "expected panic->throw codegen, got: {js}"
    );
}

#[test]
fn js_emit_handles_or_pattern() {
    // `|` alternatives in a match arm — ILO-411.
    let js = emit_js(r#"f x:t>t;?x{"a"|"b":"low";_:"high"}"#);
    // Should compile to `x === "a" || x === "b"` over the alternatives.
    assert!(
        js.contains("||"),
        "expected disjunction codegen for or-pattern, got: {js}"
    );
    assert!(
        js.contains(r#"=== "a""#) && js.contains(r#"=== "b""#),
        "expected literal comparisons in or-pattern, got: {js}"
    );
}
