// Regression coverage for `default-on-err r d` — the Result mirror of `??`.
//
// Also covers the ILO-T041 diagnostic emitted when `??` is used on a Result
// value (the single most common agent mistake per the feedback survey).
//
// What the tests cover:
//
// - Ok(v) returns the inner value (not the default)
// - Err(e) returns the default
// - num returns R n t, so `default-on-err (num s) 0` is the correct idiom
// - wrong-type default is a verifier error (ILO-T040)
// - `?? (num s) 0` triggers ILO-T041 (Result used with nil-coalesce)
// - cross-engine: VM and Cranelift JIT get the same semantics via tree-bridge

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_file(engine: &str, src: &str, entry: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "ilo_default_on_err_{}_{}.ilo",
        std::process::id(),
        seq
    ));
    std::fs::write(&path, src).unwrap();
    let out = ilo()
        .args([path.to_str().unwrap(), engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_file_expect_err(engine: &str, src: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "ilo_default_on_err_err_{}_{}.ilo",
        std::process::id(),
        seq
    ));
    std::fs::write(&path, src).unwrap();
    let out = ilo()
        .args([path.to_str().unwrap(), engine])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected verify/runtime failure but ilo {engine} succeeded for `{src}`"
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

// --- Programs under test ---

// num returns R n t; default-on-err unwraps on Ok.
const DOE_OK_NUM: &str = r#"f>n;default-on-err (num "42") 0"#;

// num on non-numeric input returns Err; default-on-err uses fallback.
const DOE_ERR_FALLBACK: &str = r#"f>n;default-on-err (num "bad") 0"#;

// Text path: Ok unwrapped.
const DOE_OK_TEXT: &str = r#"f>t;r=~"hello";default-on-err r "fallback""#;

// Text path: Err fallback.
const DOE_ERR_TEXT: &str = r#"f>t;r=^"oops";default-on-err r "fallback""#;

// Chained: result of default-on-err used in arithmetic.
const DOE_CHAIN: &str = r#"f>n;a=default-on-err (num "10") 0;b=default-on-err (num "bad") 0;+ a b"#;

fn check_all(engine: &str) {
    assert_eq!(
        run_file(engine, DOE_OK_NUM, "f"),
        "42",
        "default-on-err ok num {engine}"
    );
    assert_eq!(
        run_file(engine, DOE_ERR_FALLBACK, "f"),
        "0",
        "default-on-err err fallback {engine}"
    );
    assert_eq!(
        run_file(engine, DOE_OK_TEXT, "f"),
        "hello",
        "default-on-err ok text {engine}"
    );
    assert_eq!(
        run_file(engine, DOE_ERR_TEXT, "f"),
        "fallback",
        "default-on-err err text {engine}"
    );
    assert_eq!(
        run_file(engine, DOE_CHAIN, "f"),
        "10",
        "default-on-err chain {engine}"
    );
}

#[test]
fn default_on_err_vm() {
    // --vm exercises the register VM, which routes default-on-err through
    // the tree-bridge (is_tree_bridge_eligible(DefaultOnErr, 2) = true), so
    // this run covers both the VM dispatch and the tree-walker arm.
    check_all("--vm");
}

#[cfg(feature = "cranelift")]
#[test]
fn default_on_err_cranelift() {
    check_all("--jit");
}

// --- Verifier diagnostics ---

#[test]
fn default_on_err_wrong_default_type_rejected() {
    // R n t with default "text" — Ok type known, default mismatched. Distinct
    // from "first arg isn't a Result", so emits ILO-T042, not T040.
    let src = r#"f>n;default-on-err (num "42") "wrong""#;
    let stderr = run_file_expect_err("--vm", src);
    assert!(
        stderr.contains("ILO-T042"),
        "expected ILO-T042 for wrong default type, got: {stderr}"
    );
    assert!(
        stderr.contains("default-on-err"),
        "expected message to mention default-on-err, got: {stderr}"
    );
}

#[test]
fn default_on_err_non_result_arg_rejected() {
    // Passing a plain number (not a Result) must emit ILO-T040 (shape error).
    // Plain `n` is neither Result nor Optional, so the hint should NOT suggest
    // `??` — that path is reserved for the Optional confusion case below.
    let src = r#"f>n;default-on-err 42 0"#;
    let stderr = run_file_expect_err("--vm", src);
    assert!(
        stderr.contains("ILO-T040"),
        "expected ILO-T040 for non-result first arg, got: {stderr}"
    );
    assert!(
        !stderr.contains("use `?? v d` for Optional"),
        "hint should not steer to ?? when first arg is plain n: {stderr}"
    );
}

#[test]
fn default_on_err_optional_arg_hints_at_nil_coalesce() {
    // First arg is `O T` (Optional), not `R T E`. T040 fires AND the hint
    // should steer the agent to `??`, which is the correct unwrap for Optional.
    let src = "mk x:n>O n;>=x 1{x}\nf>n;v=mk 0;default-on-err v 99";
    let stderr = run_file_expect_err("--vm", src);
    assert!(
        stderr.contains("ILO-T040"),
        "expected ILO-T040 for Optional first arg, got: {stderr}"
    );
    assert!(
        stderr.contains("?? v d"),
        "expected hint to steer at `?? v d` for Optional, got: {stderr}"
    );
}

// --- ILO-T041: ?? on Result ---

#[test]
fn nil_coalesce_on_result_emits_ilo_t041() {
    // The classic agent mistake: `?? (num s) 0` — num returns R n t, not O n.
    // The verifier must emit ILO-T041 pointing to default-on-err.
    let src = r#"f>n;s="42";??(num s)0"#;
    let stderr = run_file_expect_err("--vm", src);
    assert!(
        stderr.contains("ILO-T041"),
        "expected ILO-T041 for ?? on Result, got: {stderr}"
    );
    assert!(
        stderr.contains("default-on-err"),
        "expected diagnostic to suggest default-on-err, got: {stderr}"
    );
}

#[test]
fn nil_coalesce_on_optional_still_works() {
    // Sanity: ?? on an Optional should not trigger ILO-T041.
    // mk returns n when guard fires, nil otherwise.
    let src = "mk x:n>n;>=x 1{x}\nf>n;v=mk 0;v??99";
    // Should run without error; no ILO-T041.
    assert_eq!(run_file("--vm", src, "f"), "99");
}

#[test]
fn nil_coalesce_on_unknown_skips_ilo_t041() {
    // T041 fires only when the lhs type is concretely `Ty::Result(..)`. If the
    // lhs is `Ty::Unknown` (e.g. an `_`-typed function param), the verifier
    // doesn't know whether it's Optional or Result, so it must NOT emit T041 —
    // a false positive there would block legitimate generic code. This pins
    // the intentional skip on the `Ty::Unknown` branch of the NilCoalesce arm.
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "ilo_default_on_err_unknown_{}_{}.ilo",
        std::process::id(),
        seq
    ));
    // `x:a` is a type variable, treated as `Ty::Unknown` by the verifier.
    std::fs::write(&path, "g x:a>a;x??0\nf>n;g 7").unwrap();
    let out = ilo()
        .args([path.to_str().unwrap(), "--vm", "f"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("ILO-T041"),
        "ILO-T041 should not fire when lhs type is Unknown: {stderr}"
    );
}
