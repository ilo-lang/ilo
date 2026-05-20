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
fn default_on_err_tree() {
    // Tree interpreter via --run-vm (tree-bridge path).
    check_all("--run-vm");
}

#[test]
fn default_on_err_vm() {
    check_all("--run-vm");
}

#[cfg(feature = "cranelift")]
#[test]
fn default_on_err_cranelift() {
    check_all("--jit");
}

// --- Verifier diagnostics ---

#[test]
fn default_on_err_wrong_default_type_rejected() {
    // R n t with default "text" must be caught by the verifier as ILO-T040.
    let src = r#"f>n;default-on-err (num "42") "wrong""#;
    let stderr = run_file_expect_err("--run-vm", src);
    assert!(
        stderr.contains("ILO-T040"),
        "expected ILO-T040 for wrong default type, got: {stderr}"
    );
    assert!(
        stderr.contains("default-on-err"),
        "expected message to mention default-on-err, got: {stderr}"
    );
}

#[test]
fn default_on_err_non_result_arg_rejected() {
    // Passing a plain number (not a Result) must emit ILO-T040.
    let src = r#"f>n;default-on-err 42 0"#;
    let stderr = run_file_expect_err("--run-vm", src);
    assert!(
        stderr.contains("ILO-T040"),
        "expected ILO-T040 for non-result first arg, got: {stderr}"
    );
}

// --- ILO-T041: ?? on Result ---

#[test]
fn nil_coalesce_on_result_emits_ilo_t041() {
    // The classic agent mistake: `?? (num s) 0` — num returns R n t, not O n.
    // The verifier must emit ILO-T041 pointing to default-on-err.
    let src = r#"f>n;s="42";??(num s)0"#;
    let stderr = run_file_expect_err("--run-vm", src);
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
    assert_eq!(run_file("--run-vm", src, "f"), "99");
}
