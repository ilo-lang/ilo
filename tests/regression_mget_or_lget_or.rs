// Regression coverage for `mget-or m k default` and `lget-or xs i default`,
// the v0.12.1 defaulted-lookup builtins. Both lower through the tree-bridge
// so VM and Cranelift share semantics with the interpreter dispatch; this
// test pins behaviour on every public engine (VM and, where feature-enabled,
// Cranelift JIT).
//
// What the tests cover:
//
// - present key/index returns the stored value (not the default)
// - missing key returns the default
// - OOB index returns the default (no runtime error, unlike `at`)
// - negative indices follow `at`'s resolution: -1 = last
// - far-negative index returns the default
// - default-type mismatch is a verify-time error (ILO-T013)

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_file(engine: &str, src: &str, entry: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path =
        std::env::temp_dir().join(format!("ilo_mget_lget_or_{}_{}.@", std::process::id(), seq));
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
        "ilo_mget_lget_or_err_{}_{}.@",
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

// --- mget-or ---

const MGET_OR_HIT: &str = r#"f>n;m=mset mmap "k" 5;mget-or m "k" 0"#;
const MGET_OR_MISS: &str = r#"f>n;m=mmap;mget-or m "missing" 99"#;
const MGET_OR_TEXT_MISS: &str = r#"f>t;m=mmap;mget-or m "k" "fallback""#;
// Numeric-key map. Confirms key-type coercion mirrors mget.
const MGET_OR_NUM_KEY_HIT: &str = r#"f>t;m=mset mmap 7 "seven";mget-or m 7 "default""#;
const MGET_OR_NUM_KEY_MISS: &str = r#"f>t;m=mset mmap 7 "seven";mget-or m 8 "default""#;

// --- lget-or ---

const LGET_OR_HIT: &str = "f>n;xs=[10,20,30];lget-or xs 1 -1";
const LGET_OR_FIRST: &str = "f>n;xs=[10,20,30];lget-or xs 0 -1";
const LGET_OR_LAST_NEG: &str = "f>n;xs=[10,20,30];lget-or xs -1 -1";
const LGET_OR_PEN_NEG: &str = "f>n;xs=[10,20,30];lget-or xs -2 -1";
const LGET_OR_OOB_HIGH: &str = "f>n;xs=[10,20,30];lget-or xs 99 -1";
const LGET_OR_OOB_NEG: &str = "f>n;xs=[10,20,30];lget-or xs -99 -1";
const LGET_OR_EMPTY: &str = "f>n;xs=[];lget-or xs 0 -1";
const LGET_OR_TEXT: &str = r#"f>t;xs=["a","b","c"];lget-or xs 5 "z""#;
// Float index auto-floors at the boundary, matching `at xs 1.7 == at xs 1`.
const LGET_OR_FLOAT_FLOOR: &str = "f>n;xs=[10,20,30];lget-or xs 1.7 -1";

fn check_all(engine: &str) {
    // mget-or
    assert_eq!(
        run_file(engine, MGET_OR_HIT, "f"),
        "5",
        "mget-or hit {engine}"
    );
    assert_eq!(
        run_file(engine, MGET_OR_MISS, "f"),
        "99",
        "mget-or miss {engine}"
    );
    assert_eq!(
        run_file(engine, MGET_OR_TEXT_MISS, "f"),
        "fallback",
        "mget-or text miss {engine}"
    );
    assert_eq!(
        run_file(engine, MGET_OR_NUM_KEY_HIT, "f"),
        "seven",
        "mget-or num-key hit {engine}"
    );
    assert_eq!(
        run_file(engine, MGET_OR_NUM_KEY_MISS, "f"),
        "default",
        "mget-or num-key miss {engine}"
    );

    // lget-or
    assert_eq!(
        run_file(engine, LGET_OR_HIT, "f"),
        "20",
        "lget-or hit {engine}"
    );
    assert_eq!(
        run_file(engine, LGET_OR_FIRST, "f"),
        "10",
        "lget-or first {engine}"
    );
    assert_eq!(
        run_file(engine, LGET_OR_LAST_NEG, "f"),
        "30",
        "lget-or -1 {engine}"
    );
    assert_eq!(
        run_file(engine, LGET_OR_PEN_NEG, "f"),
        "20",
        "lget-or -2 {engine}"
    );
    assert_eq!(
        run_file(engine, LGET_OR_OOB_HIGH, "f"),
        "-1",
        "lget-or oob high {engine}"
    );
    assert_eq!(
        run_file(engine, LGET_OR_OOB_NEG, "f"),
        "-1",
        "lget-or oob neg {engine}"
    );
    assert_eq!(
        run_file(engine, LGET_OR_EMPTY, "f"),
        "-1",
        "lget-or empty {engine}"
    );
    assert_eq!(
        run_file(engine, LGET_OR_TEXT, "f"),
        "z",
        "lget-or text {engine}"
    );
    assert_eq!(
        run_file(engine, LGET_OR_FLOAT_FLOOR, "f"),
        "20",
        "lget-or float floor {engine}"
    );
}

#[test]
fn mget_or_lget_or_tree() {
    check_all("--run-vm");
}

#[test]
fn mget_or_lget_or_vm() {
    check_all("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn mget_or_lget_or_cranelift() {
    check_all("--jit");
}

// Default-type mismatch must be caught by the verifier with ILO-T013, not
// silently coerced or deferred to runtime — this is the manifesto win, the
// agent gets the signal in-context rather than re-running on a wrong answer.

#[test]
fn mget_or_default_type_mismatch_rejected() {
    // Map is M t n (value=n); default is text "x" — must error.
    let src = r#"f>n;m=mset mmap "k" 5;mget-or m "k" "x""#;
    let stderr = run_file_expect_err("--run-vm", src);
    assert!(
        stderr.contains("ILO-T013"),
        "expected ILO-T013 for mget-or wrong default type, got: {stderr}"
    );
    assert!(
        stderr.contains("mget-or"),
        "expected message to mention mget-or, got: {stderr}"
    );
}

#[test]
fn lget_or_default_type_mismatch_rejected() {
    // List is L n; default is text "x" — must error.
    let src = r#"f>n;xs=[10,20,30];lget-or xs 1 "x""#;
    let stderr = run_file_expect_err("--run-vm", src);
    assert!(
        stderr.contains("ILO-T013"),
        "expected ILO-T013 for lget-or wrong default type, got: {stderr}"
    );
    assert!(
        stderr.contains("lget-or"),
        "expected message to mention lget-or, got: {stderr}"
    );
}
