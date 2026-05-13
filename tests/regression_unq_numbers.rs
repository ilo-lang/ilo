// Regression tests for `unq xs` on number lists. The original report from
// edu-teacher's 2026-05-11 Anscombe's Quartet session (lines 391/586 of
// ilo_assessment_feedback.md) said `len unq xs` hung with exit 137 / OOM
// on a meaningfully sized `L n`. The root cause was raw-bits comparison
// for list dedup — fixed in commit 5a30e00 (2026-03-06) which switched
// the list path to `nanval_equal`. The persona was running a stale
// binary at the time of the report (confirmed by their own entry #9 in
// the same batch about needing to reinstall an old binary).
//
// These tests lock in the correct behaviour across all three engines so
// any future regression to raw-bits comparison, RC mishandling, or
// quadratic blowup gets caught immediately.

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

// Basic: the exact persona repro — `len unq xs` on [1,2,2,3] returns 3.
const BASIC_LEN_SRC: &str = "f xs:L n>n;len unq xs";

fn check_basic_len(engine: &str) {
    let out = ilo()
        .args([BASIC_LEN_SRC, engine, "f", "[1,2,2,3]"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "3",
        "engine={engine}"
    );
}

#[test]
fn unq_numbers_basic_len_tree() {
    check_basic_len("--run-tree");
}

#[test]
fn unq_numbers_basic_len_vm() {
    check_basic_len("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn unq_numbers_basic_len_cranelift() {
    check_basic_len("--run-cranelift");
}

// Return-the-list form: `unq [1,2,2,3]` returns [1, 2, 3] preserving order.
const ORDER_SRC: &str = "f>L n;unq [1, 2, 2, 3]";

fn check_order(engine: &str) {
    assert_eq!(run(engine, ORDER_SRC, "f"), "[1, 2, 3]", "engine={engine}");
}

#[test]
fn unq_numbers_preserves_order_tree() {
    check_order("--run-tree");
}

#[test]
fn unq_numbers_preserves_order_vm() {
    check_order("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn unq_numbers_preserves_order_cranelift() {
    check_order("--run-cranelift");
}

// All-same: a list of identical numbers dedupes to one element.
const ALL_SAME_SRC: &str = "f>L n;unq [7, 7, 7, 7, 7]";

fn check_all_same(engine: &str) {
    assert_eq!(run(engine, ALL_SAME_SRC, "f"), "[7]", "engine={engine}");
}

#[test]
fn unq_numbers_all_same_tree() {
    check_all_same("--run-tree");
}

#[test]
fn unq_numbers_all_same_vm() {
    check_all_same("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn unq_numbers_all_same_cranelift() {
    check_all_same("--run-cranelift");
}

// Empty list: edge case, returns [].
const EMPTY_SRC: &str = "f>L n;unq []";

fn check_empty(engine: &str) {
    assert_eq!(run(engine, EMPTY_SRC, "f"), "[]", "engine={engine}");
}

#[test]
fn unq_numbers_empty_tree() {
    check_empty("--run-tree");
}

#[test]
fn unq_numbers_empty_vm() {
    check_empty("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn unq_numbers_empty_cranelift() {
    check_empty("--run-cranelift");
}

// All-unique: no element should be dropped, length equals input length.
const ALL_UNIQUE_SRC: &str = "f xs:L n>n;len unq xs";

fn check_all_unique(engine: &str) {
    let out = ilo()
        .args([ALL_UNIQUE_SRC, engine, "f", "1,2,3,4,5,6,7,8,9,10"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "10",
        "engine={engine}"
    );
}

#[test]
fn unq_numbers_all_unique_tree() {
    check_all_unique("--run-tree");
}

#[test]
fn unq_numbers_all_unique_vm() {
    check_all_unique("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn unq_numbers_all_unique_cranelift() {
    check_all_unique("--run-cranelift");
}

// Floats: `nanval_equal` uses `f64::EPSILON` so exact-representation
// floats dedupe correctly. 1.5 and 2.5 are exact in IEEE 754.
const FLOATS_SRC: &str = "f>L n;unq [1.5, 2.5, 1.5, 2.5, 1.5]";

fn check_floats(engine: &str) {
    assert_eq!(
        run(engine, FLOATS_SRC, "f"),
        "[1.5, 2.5]",
        "engine={engine}"
    );
}

#[test]
fn unq_numbers_floats_tree() {
    check_floats("--run-tree");
}

#[test]
fn unq_numbers_floats_vm() {
    check_floats("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn unq_numbers_floats_cranelift() {
    check_floats("--run-cranelift");
}

// Negatives and zero: sign and zero are preserved through the
// equality predicate. 0 == 0 and -3 == -3 dedupe; 0 and -0 are
// equal under IEEE 754 subtraction comparison and should collapse.
const NEGATIVES_SRC: &str = "f>L n;unq [0, -3, 5, -3, 0, 5]";

fn check_negatives(engine: &str) {
    assert_eq!(
        run(engine, NEGATIVES_SRC, "f"),
        "[0, -3, 5]",
        "engine={engine}"
    );
}

#[test]
fn unq_numbers_negatives_tree() {
    check_negatives("--run-tree");
}

#[test]
fn unq_numbers_negatives_vm() {
    check_negatives("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn unq_numbers_negatives_cranelift() {
    check_negatives("--run-cranelift");
}

// Stress test: 1000-element list (500 unique values, each repeated
// twice in interleaved order). Catches quadratic-blowup OOM and any
// silent miscompile that would let the dedup count drift. Runs well
// under a second on all three engines on a normal laptop.
//
// We construct the input as a CLI list arg "1,1,2,2,3,3,...,500,500".
fn stress_input() -> String {
    let mut parts: Vec<String> = Vec::with_capacity(2000);
    for i in 1..=500 {
        parts.push(i.to_string());
        parts.push(i.to_string());
    }
    parts.join(",")
}

const STRESS_SRC: &str = "f xs:L n>n;len unq xs";

fn check_stress(engine: &str) {
    let input = stress_input();
    let out = ilo()
        .args([STRESS_SRC, engine, "f", &input])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed on 1000-elem stress: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "500",
        "engine={engine}"
    );
}

#[test]
fn unq_numbers_stress_1000_tree() {
    check_stress("--run-tree");
}

#[test]
fn unq_numbers_stress_1000_vm() {
    check_stress("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn unq_numbers_stress_1000_cranelift() {
    check_stress("--run-cranelift");
}
