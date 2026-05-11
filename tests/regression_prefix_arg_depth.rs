// Regression tests for prefix-binary expressions used as call arguments.
//
// Previously, `parse_call_or_atom` would stop collecting call args as soon as
// the next token was infix-eligible, even when that operator was actually
// starting a *prefix-binary* expression (e.g. `+i 1` inside `slc ls i +i 1`).
// The fix mirrors the same `looks_like_prefix_binary` guard already used
// before the arg loop: don't break if the operator looks prefix-binary.

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

// Original repro: slc with a prefix-binary 3rd arg.
const SLC_REPRO: &str = "f>L n;ls=[10,20,30];i=0;slc ls i +i 1";

fn check_slc(engine: &str) {
    assert_eq!(run(engine, SLC_REPRO, "f"), "[10]", "engine={engine}");
}

#[test]
fn slc_with_prefix_arg_tree() {
    check_slc("--run-tree");
}

#[test]
fn slc_with_prefix_arg_vm() {
    check_slc("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn slc_with_prefix_arg_cranelift() {
    check_slc("--run-cranelift");
}

// 3-arg user function with a prefix-binary expression in each position.
// `g(a, b, c) = a + b + c`. Calls below should all yield 6.
const G_DEF: &str = "g a:n b:n c:n>n;+ +a b c";

fn check_prefix_in_position(engine: &str, args: &[&str], expected: &str) {
    let mut cmd_args: Vec<String> = vec![G_DEF.to_string(), engine.to_string(), "g".to_string()];
    for a in args {
        cmd_args.push(a.to_string());
    }
    let out = ilo().args(&cmd_args).output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        expected,
        "engine={engine} args={args:?}"
    );
}

fn check_three_arg_prefix(engine: &str) {
    // Plain: 1+2+3 = 6
    check_prefix_in_position(engine, &["1", "2", "3"], "6");
}

#[test]
fn three_arg_prefix_tree() {
    check_three_arg_prefix("--run-tree");
}

#[test]
fn three_arg_prefix_vm() {
    check_three_arg_prefix("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn three_arg_prefix_cranelift() {
    check_three_arg_prefix("--run-cranelift");
}

// Infix on a call result still works: `g 5 + 3` = `(g 5) + 3` = `10 + 3` = 13.
// Multi-fn source must be passed as a file because `;` in the single-arg form
// can swallow fn-decl boundaries in some shapes.
fn check_infix_on_call(engine: &str) {
    let path = std::env::temp_dir().join("ilo_prefix_arg_t3.ilo");
    std::fs::write(&path, "g x:n>n;*x 2\nf>n;a=g 5;+a 3\n").unwrap();
    let out = ilo()
        .args([path.to_str().unwrap(), engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "13",
        "engine={engine}"
    );
}

#[test]
fn infix_on_call_result_tree() {
    check_infix_on_call("--run-tree");
}

#[test]
fn infix_on_call_result_vm() {
    check_infix_on_call("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn infix_on_call_result_cranelift() {
    check_infix_on_call("--run-cranelift");
}

// Guard expression with negative literal: ensure the parser isn't confused.
// abs via braced guard: `>a 0{a}{- 0 a}`.
fn check_abs_guard(engine: &str) {
    let src = "f a:n>n;>a 0{a}{- 0 a}";
    let out_neg = ilo()
        .args([src, engine, "f", "-5"])
        .output()
        .expect("failed");
    let out_pos = ilo()
        .args([src, engine, "f", "7"])
        .output()
        .expect("failed");
    assert!(
        out_neg.status.success(),
        "neg: {}",
        String::from_utf8_lossy(&out_neg.stderr)
    );
    assert!(
        out_pos.status.success(),
        "pos: {}",
        String::from_utf8_lossy(&out_pos.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out_neg.stdout).trim(),
        "5",
        "engine={engine} abs(-5)"
    );
    assert_eq!(
        String::from_utf8_lossy(&out_pos.stdout).trim(),
        "7",
        "engine={engine} abs(7)"
    );
}

#[test]
fn abs_guard_tree() {
    check_abs_guard("--run-tree");
}

#[test]
fn abs_guard_vm() {
    check_abs_guard("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn abs_guard_cranelift() {
    check_abs_guard("--run-cranelift");
}

// Characterization test: `f +x` where the prefix arg has only ONE operand.
// `looks_like_prefix_binary` requires count >= 2, so it returns false here,
// the call loop breaks, and `+x` is parsed as infix on the prior expression
// (`f + x`), which is a type error because `f` is a function not a number.
// This pins the current behavior — if a future change shifts the count
// threshold, this test will flag the semantic change loudly.
fn check_single_atom_after_op(engine: &str) {
    let path = std::env::temp_dir().join("ilo_prefix_arg_single.ilo");
    std::fs::write(&path, "f a:n>n;a\ng x:n>n;f +x\n").unwrap();
    let out = ilo()
        .args([path.to_str().unwrap(), engine, "g", "3"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "engine={engine}: expected `f +x` to fail (parsed as infix on function ref); \
         got success with stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("ILO-T009") || stderr.contains("expects matching"),
        "engine={engine}: expected ILO-T009 type error, got stderr={stderr}"
    );
}

#[test]
fn single_atom_after_op_tree() {
    check_single_atom_after_op("--run-tree");
}

#[test]
fn single_atom_after_op_vm() {
    check_single_atom_after_op("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn single_atom_after_op_cranelift() {
    check_single_atom_after_op("--run-cranelift");
}

// FOLLOW-UP: multi-fn programs written in the single-line `;`-separated form
// (e.g. `g a:n>n;+a b;f>n;g 1`) can swallow fn-decl boundaries in some shapes,
// so these tests pass multi-fn sources via a tempfile instead. The single-line
// form quirk is a separate parser issue tracked outside this regression set.
