// Regression tests for the `at xs i` builtin — i-th element of a list or text.
//
// Out-of-range / wrong-type is a runtime error on every engine: tree, VM, and
// cranelift JIT. Cranelift's helper sets a thread-local error cell which the
// JIT entry point picks up and surfaces as a `VmRuntimeError`, matching the
// behaviour tree and VM already had. Prior to that fix, cranelift's helper
// silently returned `TAG_NIL` on every failure mode.
//
// Float indices auto-floor at the `at` boundary (was: strict integer guard).
// `at xs 1.7 == at xs 1`, `at xs -0.5` floors to `-1` (last element), etc.
// Non-numeric still errors. See the FLOAT_* / NEG_*_FLOAT / COMPUTED_FLOAT
// cases below for cross-engine coverage.

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

// Basic: index into a list of numbers.
const NUM_SRC: &str = "f>n;xs=[10,20,30];at xs 1";

fn check_num_index(engine: &str) {
    assert_eq!(run(engine, NUM_SRC, "f"), "20", "engine={engine}");
}

#[test]
fn at_num_index_tree() {
    check_num_index("--run-tree");
}

#[test]
fn at_num_index_vm() {
    check_num_index("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn at_num_index_cranelift() {
    check_num_index("--run-cranelift");
}

// Type variable: works on a list of text too.
const TEXT_SRC: &str = "f>t;xs=[\"a\",\"b\",\"c\"];at xs 2";

fn check_text_index(engine: &str) {
    assert_eq!(run(engine, TEXT_SRC, "f"), "c", "engine={engine}");
}

#[test]
fn at_text_index_tree() {
    check_text_index("--run-tree");
}

#[test]
fn at_text_index_vm() {
    check_text_index("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn at_text_index_cranelift() {
    check_text_index("--run-cranelift");
}

// First element with index 0.
const FIRST_SRC: &str = "f>n;xs=[10,20,30];at xs 0";

fn check_first(engine: &str) {
    assert_eq!(run(engine, FIRST_SRC, "f"), "10", "engine={engine}");
}

#[test]
fn at_first_tree() {
    check_first("--run-tree");
}

#[test]
fn at_first_vm() {
    check_first("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn at_first_cranelift() {
    check_first("--run-cranelift");
}

// Last element of a 3-element list via hardcoded index.
const LAST_SRC: &str = "f>n;xs=[10,20,30];at xs 2";

fn check_last(engine: &str) {
    assert_eq!(run(engine, LAST_SRC, "f"), "30", "engine={engine}");
}

#[test]
fn at_last_tree() {
    check_last("--run-tree");
}

#[test]
fn at_last_vm() {
    check_last("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn at_last_cranelift() {
    check_last("--run-cranelift");
}

// Out-of-range: every engine (tree, VM, cranelift) raises a runtime error.
// Accepts ILO-R004 (cranelift, where the JIT helper sets the TLS error cell
// and the entry point synthesises a VmError::Type) or ILO-R009 (tree, where
// the interpreter raises RuntimeError directly).
const OOR_SRC: &str = "f>n;xs=[10,20,30];at xs 99";

fn check_oor_error(engine: &str) {
    let out = ilo()
        .args([OOR_SRC, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "engine={engine}: expected runtime error for at xs 99, got stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("at")
            || stderr.contains("range")
            || stderr.contains("ILO-R009")
            || stderr.contains("ILO-R004"),
        "engine={engine}: expected at/range error, got stderr={stderr}"
    );
}

#[test]
fn at_out_of_range_tree() {
    check_oor_error("--run-tree");
}

#[test]
fn at_out_of_range_vm() {
    check_oor_error("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn at_out_of_range_cranelift() {
    check_oor_error("--run-cranelift");
}

// Negative index: Python-style from-the-end indexing.
// -1 = last element, -2 = second-to-last, etc.
const NEG_LAST_SRC: &str = "f>n;xs=[10,20,30];at xs -1";

fn check_neg_last(engine: &str) {
    assert_eq!(run(engine, NEG_LAST_SRC, "f"), "30", "engine={engine}");
}

#[test]
fn at_negative_last_tree() {
    check_neg_last("--run-tree");
}

#[test]
fn at_negative_last_vm() {
    check_neg_last("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn at_negative_last_cranelift() {
    check_neg_last("--run-cranelift");
}

// -3 on a 3-element list reaches the first element.
const NEG_FIRST_SRC: &str = "f>n;xs=[10,20,30];at xs -3";

fn check_neg_first(engine: &str) {
    assert_eq!(run(engine, NEG_FIRST_SRC, "f"), "10", "engine={engine}");
}

#[test]
fn at_negative_first_tree() {
    check_neg_first("--run-tree");
}

#[test]
fn at_negative_first_vm() {
    check_neg_first("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn at_negative_first_cranelift() {
    check_neg_first("--run-cranelift");
}

// Negative index on text: -1 yields the last character.
const NEG_TEXT_SRC: &str = "f>t;xs=[\"a\",\"b\",\"c\"];at xs -1";

fn check_neg_text(engine: &str) {
    assert_eq!(run(engine, NEG_TEXT_SRC, "f"), "c", "engine={engine}");
}

#[test]
fn at_negative_text_tree() {
    check_neg_text("--run-tree");
}

#[test]
fn at_negative_text_vm() {
    check_neg_text("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn at_negative_text_cranelift() {
    check_neg_text("--run-cranelift");
}

// Out-of-range negative: -4 on a 3-element list errors on every engine.
const NEG_OOR_SRC: &str = "f>n;xs=[10,20,30];at xs -4";

fn check_neg_oor_error(engine: &str) {
    let out = ilo()
        .args([NEG_OOR_SRC, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "engine={engine}: expected runtime error for at xs -4, got stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("at")
            || stderr.contains("range")
            || stderr.contains("ILO-R009")
            || stderr.contains("ILO-R004"),
        "engine={engine}: expected at/range error, got stderr={stderr}"
    );
}

#[test]
fn at_negative_oor_tree() {
    check_neg_oor_error("--run-tree");
}

#[test]
fn at_negative_oor_vm() {
    check_neg_oor_error("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn at_negative_oor_cranelift() {
    check_neg_oor_error("--run-cranelift");
}

// Float indices auto-floor at the `at` boundary. Removes the `flr (/ ln 2)`
// ceremony when indexing with computed floats (e.g. `at xs (/ len 2)` for the
// median bucket). Floor goes toward negative infinity, so negative fractional
// indices land on the floor toward more-negative — `-0.5` floors to `-1` (last
// element after negative-index resolution).

// `at xs 1.0` — explicitly-typed float at an integer value.
const FLOAT_INTEGRAL_SRC: &str = "f>n;xs=[10,20,30];at xs 1.0";
fn check_float_integral(engine: &str) {
    assert_eq!(
        run(engine, FLOAT_INTEGRAL_SRC, "f"),
        "20",
        "engine={engine}"
    );
}

#[test]
fn at_float_integral_tree() {
    check_float_integral("--run-tree");
}

#[test]
fn at_float_integral_vm() {
    check_float_integral("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn at_float_integral_cranelift() {
    check_float_integral("--run-cranelift");
}

// `at xs 1.7` floors to 1.
const FLOAT_FRAC_SRC: &str = "f>n;xs=[10,20,30];at xs 1.7";
fn check_float_frac(engine: &str) {
    assert_eq!(run(engine, FLOAT_FRAC_SRC, "f"), "20", "engine={engine}");
}

#[test]
fn at_float_frac_tree() {
    check_float_frac("--run-tree");
}

#[test]
fn at_float_frac_vm() {
    check_float_frac("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn at_float_frac_cranelift() {
    check_float_frac("--run-cranelift");
}

// `at xs -1.5` on a len-3 list: floor(-1.5) = -2 → middle element (20).
const NEG_FLOAT_SRC: &str = "f>n;xs=[10,20,30];at xs -1.5";
fn check_neg_float(engine: &str) {
    assert_eq!(run(engine, NEG_FLOAT_SRC, "f"), "20", "engine={engine}");
}

#[test]
fn at_negative_float_tree() {
    check_neg_float("--run-tree");
}

#[test]
fn at_negative_float_vm() {
    check_neg_float("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn at_negative_float_cranelift() {
    check_neg_float("--run-cranelift");
}

// `at xs -0.5` on a len-3 list: floor(-0.5) = -1 → last element (30).
// (Was the previous "negative-float must error" case; now positive coverage.)
const NEG_HALF_SRC: &str = "f>n;xs=[10,20,30];at xs -0.5";
fn check_neg_half(engine: &str) {
    assert_eq!(run(engine, NEG_HALF_SRC, "f"), "30", "engine={engine}");
}

#[test]
fn at_negative_half_tree() {
    check_neg_half("--run-tree");
}

#[test]
fn at_negative_half_vm() {
    check_neg_half("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn at_negative_half_cranelift() {
    check_neg_half("--run-cranelift");
}

// `at xs (len/2)` — the originating idiom from html-scraper rerun3 (median
// bucket). `len xs` is 5, `/ k 2` is 2.5, auto-floor → 2 → returns 30 without
// a `flr` wrap. (Bind-first because `/ len xs 2` would parse as a 3-arg call
// to `/` due to parser greediness — unrelated to this fix.)
const COMPUTED_FLOAT_SRC: &str = "f>n;xs=[10,20,30,40,50];k=len xs;i=/ k 2;at xs i";
fn check_computed_float(engine: &str) {
    assert_eq!(
        run(engine, COMPUTED_FLOAT_SRC, "f"),
        "30",
        "engine={engine}"
    );
}

#[test]
fn at_computed_float_tree() {
    check_computed_float("--run-tree");
}

#[test]
fn at_computed_float_vm() {
    check_computed_float("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn at_computed_float_cranelift() {
    check_computed_float("--run-cranelift");
}

// Non-numeric index still errors — the type guard is preserved.
const NON_NUMERIC_SRC: &str = "f>n;xs=[10,20,30];at xs \"a\"";
fn check_non_numeric_error(engine: &str) {
    let out = ilo()
        .args([NON_NUMERIC_SRC, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "engine={engine}: expected error for non-numeric index, got stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("number")
            || stderr.contains("at")
            || stderr.contains("ILO-R009")
            || stderr.contains("ILO-T013"),
        "engine={engine}: expected at/number error, got stderr={stderr}"
    );
}

#[test]
fn at_non_numeric_tree() {
    check_non_numeric_error("--run-tree");
}

#[test]
fn at_non_numeric_vm() {
    check_non_numeric_error("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn at_non_numeric_cranelift() {
    check_non_numeric_error("--run-cranelift");
}
