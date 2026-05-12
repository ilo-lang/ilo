// Regression tests for the `range a b` builtin — half-open integer range [a, b).
//
// Returns L n. Empty when a >= b. Cross-engine: tree, vm, cranelift.

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

// Basic: range 0 5 → [0,1,2,3,4]
const BASIC_SRC: &str = "f>L n;range 0 5";

fn check_basic(engine: &str) {
    assert_eq!(
        run(engine, BASIC_SRC, "f"),
        "[0, 1, 2, 3, 4]",
        "engine={engine}"
    );
}

#[test]
fn range_basic_tree() {
    check_basic("--run-tree");
}

#[test]
fn range_basic_vm() {
    check_basic("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn range_basic_cranelift() {
    check_basic("--run-cranelift");
}

// Empty: a == b
const EMPTY_SRC: &str = "f>L n;range 3 3";

fn check_empty(engine: &str) {
    assert_eq!(run(engine, EMPTY_SRC, "f"), "[]", "engine={engine}");
}

#[test]
fn range_empty_tree() {
    check_empty("--run-tree");
}

#[test]
fn range_empty_vm() {
    check_empty("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn range_empty_cranelift() {
    check_empty("--run-cranelift");
}

// Flipped: a > b → empty (not error)
const FLIPPED_SRC: &str = "f>L n;range 5 3";

fn check_flipped(engine: &str) {
    assert_eq!(run(engine, FLIPPED_SRC, "f"), "[]", "engine={engine}");
}

#[test]
fn range_flipped_tree() {
    check_flipped("--run-tree");
}

#[test]
fn range_flipped_vm() {
    check_flipped("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn range_flipped_cranelift() {
    check_flipped("--run-cranelift");
}

// Negative start: range -2 3 → [-2,-1,0,1,2]
const NEG_SRC: &str = "f>L n;range -2 3";

fn check_neg(engine: &str) {
    assert_eq!(
        run(engine, NEG_SRC, "f"),
        "[-2, -1, 0, 1, 2]",
        "engine={engine}"
    );
}

#[test]
fn range_neg_tree() {
    check_neg("--run-tree");
}

#[test]
fn range_neg_vm() {
    check_neg("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn range_neg_cranelift() {
    check_neg("--run-cranelift");
}

// Gauss check: sum of range 0..11 → 55. Tree-only because the inline-CLI
// pre-existing path for `sum xs` over a builtin-call expression isn't wired
// for vm/cranelift in this entry shape; the cross-engine VM/cranelift coverage
// is provided by the explicit list-equality tests above.
const GAUSS_SRC: &str = "f>n;sum (range 0 11)";

fn check_gauss(engine: &str) {
    assert_eq!(run(engine, GAUSS_SRC, "f"), "55", "engine={engine}");
}

#[test]
fn range_gauss_tree() {
    check_gauss("--run-tree");
}

// Fractional bounds must error rather than silently truncate. `range 1.9 4.9`
// previously yielded `[1,2,3]` via `as i64` truncation; now both engines
// reject non-integer bounds. (The cranelift JIT falls back to its existing
// silent-nil-on-bad-input pattern, consistent with other builtins.)
const FRAC_SRC: &str = "f>L n;range 1.9 4.9";

fn check_frac_errors(engine: &str) {
    let out = ilo()
        .args([FRAC_SRC, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "engine={engine}: expected range 1.9 4.9 to error, got stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("integer") || err.contains("bounds"),
        "engine={engine}: stderr={err}"
    );
}

#[test]
fn range_fractional_bounds_error_tree() {
    check_frac_errors("--run-tree");
}

#[test]
fn range_fractional_bounds_error_vm() {
    check_frac_errors("--run-vm");
}
