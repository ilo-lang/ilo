// Regression tests for the `lst xs i v` builtin — functional list update.
//
// Semantics:
//   - Returns a new list with element at index `i` replaced by `v`.
//   - The original list is never mutated (value semantics).
//   - Type variable: list element type and replacement value must match.
//   - Out-of-range / negative index:
//       * tree and vm engines raise ILO-R004 / ILO-R009 runtime errors.
//       * cranelift JIT returns the original list unchanged (no error).
//     This mirrors `at`'s pattern of tree/vm strict, JIT permissive.

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

// Basic: replace middle element of a numeric list.
const NUM_SRC: &str = "f>L n;lst [10,20,30] 1 99";

fn check_num(engine: &str) {
    assert_eq!(run(engine, NUM_SRC, "f"), "[10, 99, 30]", "engine={engine}");
}

#[test]
fn lst_num_tree() {
    check_num("--run-tree");
}

#[test]
fn lst_num_vm() {
    check_num("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn lst_num_cranelift() {
    check_num("--run-cranelift");
}

// Type variable: works on a list of text too, with same-type replacement.
const TEXT_SRC: &str = "f>L t;lst [\"a\",\"b\",\"c\"] 2 \"X\"";

fn check_text(engine: &str) {
    assert_eq!(run(engine, TEXT_SRC, "f"), "[a, b, X]", "engine={engine}");
}

#[test]
fn lst_text_tree() {
    check_text("--run-tree");
}

#[test]
fn lst_text_vm() {
    check_text("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn lst_text_cranelift() {
    check_text("--run-cranelift");
}

// First and last indices.
#[test]
fn lst_first_tree() {
    assert_eq!(
        run("--run-tree", "f>L n;lst [10,20,30] 0 99", "f"),
        "[99, 20, 30]"
    );
}

#[test]
fn lst_first_vm() {
    assert_eq!(
        run("--run-vm", "f>L n;lst [10,20,30] 0 99", "f"),
        "[99, 20, 30]"
    );
}

#[test]
fn lst_last_tree() {
    assert_eq!(
        run("--run-tree", "f>L n;lst [10,20,30] 2 99", "f"),
        "[10, 20, 99]"
    );
}

#[test]
fn lst_last_vm() {
    assert_eq!(
        run("--run-vm", "f>L n;lst [10,20,30] 2 99", "f"),
        "[10, 20, 99]"
    );
}

// Original list is unchanged after lst returns a new list.
const NO_MUT_SRC: &str = "f>L n;xs=[1,2,3];ys=lst xs 0 99;xs";

fn check_no_mut(engine: &str) {
    assert_eq!(run(engine, NO_MUT_SRC, "f"), "[1, 2, 3]", "engine={engine}");
}

#[test]
fn lst_no_mutation_tree() {
    check_no_mut("--run-tree");
}

#[test]
fn lst_no_mutation_vm() {
    check_no_mut("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn lst_no_mutation_cranelift() {
    check_no_mut("--run-cranelift");
}

// Out-of-range: tree and vm engines raise a runtime error.
const OOR_SRC: &str = "f>L n;lst [10,20,30] 99 42";

fn check_oor_error(engine: &str) {
    let out = ilo()
        .args([OOR_SRC, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "engine={engine}: expected runtime error, got stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("lst") || stderr.contains("range") || stderr.contains("ILO-R"),
        "engine={engine}: expected lst/range/ILO-R error, got stderr={stderr}"
    );
}

#[test]
fn lst_out_of_range_tree() {
    check_oor_error("--run-tree");
}

#[test]
fn lst_out_of_range_vm() {
    check_oor_error("--run-vm");
}

// Cranelift JIT mirrors `at`'s permissive pattern: returns the original list
// unchanged on out-of-range. Safer than nil because the caller can still chain.
#[test]
#[cfg(feature = "cranelift")]
fn lst_out_of_range_cranelift() {
    let out = ilo()
        .args([OOR_SRC, "--run-cranelift", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "cranelift: expected success returning original list, got stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("[10, 20, 30]"),
        "cranelift: expected original list [10, 20, 30], got {stdout}"
    );
}

// Negative index: tree/vm error, cranelift returns original list unchanged.
const NEG_SRC: &str = "f>L n;lst [10,20,30] -1 42";

#[test]
fn lst_negative_tree() {
    let out = ilo()
        .args([NEG_SRC, "--run-tree", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "tree: expected error for lst xs -1 v, got stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("non-negative") || stderr.contains("ILO-R"),
        "tree: expected non-negative/ILO-R error, got stderr={stderr}"
    );
}

#[test]
fn lst_negative_vm() {
    let out = ilo()
        .args([NEG_SRC, "--run-vm", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "vm: expected error for lst xs -1 v, got stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("non-negative") || stderr.contains("lst") || stderr.contains("ILO-R"),
        "vm: expected non-negative/lst/ILO-R error, got stderr={stderr}"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn lst_negative_cranelift() {
    let out = ilo()
        .args([NEG_SRC, "--run-cranelift", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "cranelift: expected success returning original list, got stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("[10, 20, 30]"),
        "cranelift: expected original list, got {stdout}"
    );
}

// Type-variable enforcement: a text value in a numeric list is a verifier error.
#[test]
fn lst_type_mismatch_rejected() {
    let out = ilo()
        .args(["f>L n;lst [10,20,30] 1 \"X\"", "--run-tree", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected verifier error for type mismatch, got stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("ILO-T013") || stderr.contains("does not match"),
        "expected ILO-T013, got stderr={stderr}"
    );
}

// Histogram bin-update shape — uses `lst` inside a loop to increment bins.
// Exercises the value-semantics rebuild path that earthquake percentiles and
// flight-delay histograms previously had to do in user space.
const HISTOGRAM_SRC: &str = "f>L n;bins=[0,0,0,0];samples=[0,2,1,2,3,1,2,0];@s samples{c=at bins s;bins=lst bins s +c 1};bins";

fn check_histogram(engine: &str) {
    // 0 appears 2x, 1 appears 2x, 2 appears 3x, 3 appears 1x.
    assert_eq!(
        run(engine, HISTOGRAM_SRC, "f"),
        "[2, 2, 3, 1]",
        "engine={engine}"
    );
}

#[test]
fn lst_histogram_tree() {
    check_histogram("--run-tree");
}

#[test]
fn lst_histogram_vm() {
    check_histogram("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn lst_histogram_cranelift() {
    check_histogram("--run-cranelift");
}
