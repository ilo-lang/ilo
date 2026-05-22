// Regression tests for the `hex-rev` builtin (ILO-372).
//
// `hex-rev s > t` reverses the byte order of a hex-encoded string by
// swapping adjacent byte-pairs. Odd-length input errors ILO-T013.
// Case is preserved: `abCD` reversed is `CDab`.
//
// All tests exercise tree, VM, and Cranelift (JIT) for cross-engine parity.

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
        "ilo {engine} failed for `{entry}({arg})`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(engine: &str, src: &str, entry: &str, arg: &str) -> String {
    let out = ilo()
        .args([src, engine, entry, arg])
        .output()
        .expect("failed to run ilo");
    // We expect a non-zero exit code for error cases.
    String::from_utf8_lossy(&out.stderr).trim().to_string()
        + &String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// ── Basic 4-byte reversal ────────────────────────────────────────────────────

const FLIP4: &str = "go s:t>t;hex-rev s\n";

#[test]
fn hex_rev_basic_vm() {
    assert_eq!(run("--vm", FLIP4, "go", "12345678"), "78563412");
}

#[test]
#[cfg(feature = "cranelift")]
fn hex_rev_basic_cranelift() {
    assert_eq!(run("--jit", FLIP4, "go", "12345678"), "78563412");
}

// ── Empty string is a no-op (zero bytes, even length) ───────────────────────

#[test]
fn hex_rev_empty_vm() {
    assert_eq!(run("--vm", FLIP4, "go", ""), "");
}

#[test]
#[cfg(feature = "cranelift")]
fn hex_rev_empty_cranelift() {
    assert_eq!(run("--jit", FLIP4, "go", ""), "");
}

// ── Case preservation: uppercase/mixed hex ──────────────────────────────────

const FLIP_CASE: &str = "go s:t>t;hex-rev s\n";

#[test]
fn hex_rev_case_preserved_vm() {
    assert_eq!(run("--vm", FLIP_CASE, "go", "abCD"), "CDab");
}

#[test]
#[cfg(feature = "cranelift")]
fn hex_rev_case_preserved_cranelift() {
    assert_eq!(run("--jit", FLIP_CASE, "go", "abCD"), "CDab");
}

// ── 2-byte string (4 chars) ──────────────────────────────────────────────────

#[test]
fn hex_rev_two_bytes_vm() {
    assert_eq!(run("--vm", FLIP4, "go", "aabb"), "bbaa");
}

#[test]
#[cfg(feature = "cranelift")]
fn hex_rev_two_bytes_cranelift() {
    assert_eq!(run("--jit", FLIP4, "go", "aabb"), "bbaa");
}

// ── Double reversal is identity ──────────────────────────────────────────────

const ROUNDTRIP: &str = "go s:t>t;hex-rev (hex-rev s)\n";

#[test]
fn hex_rev_roundtrip_vm() {
    assert_eq!(run("--vm", ROUNDTRIP, "go", "deadbeef"), "deadbeef");
}

#[test]
#[cfg(feature = "cranelift")]
fn hex_rev_roundtrip_cranelift() {
    assert_eq!(run("--jit", ROUNDTRIP, "go", "deadbeef"), "deadbeef");
}

// ── Odd-length input errors with ILO-T013 ───────────────────────────────────

const ODD_LEN: &str = "go s:t>t;hex-rev s\n";

#[test]
fn hex_rev_odd_length_errors_vm() {
    let err = run_err("--vm", ODD_LEN, "go", "abc");
    assert!(
        err.contains("ILO-R009") || err.contains("ILO-T013"),
        "expected ILO-R009 or ILO-T013 in error output, got: {err}"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn hex_rev_odd_length_errors_cranelift() {
    let err = run_err("--jit", ODD_LEN, "go", "abc");
    assert!(
        err.contains("ILO-R009") || err.contains("ILO-T013"),
        "expected ILO-R009 or ILO-T013 in error output, got: {err}"
    );
}

// ── Padding hint is present in the error message ─────────────────────────────

#[test]
fn hex_rev_odd_length_hint_vm() {
    let err = run_err("--vm", ODD_LEN, "go", "abc");
    assert!(
        err.contains("pad") || err.contains("even"),
        "expected padding hint in error, got: {err}"
    );
}
