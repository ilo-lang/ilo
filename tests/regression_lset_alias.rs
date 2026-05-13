// Regression tests for the `lset` alias of `lst xs i v`.
//
// `lset` is a discoverability alias added because multiple personas reach for
// it from the `mset`/`lset` mental model (L↔M parallelism). It resolves to
// the canonical `lst` at the AST level, so all three engines (tree, VM,
// Cranelift) execute identical bytecode/opcodes. These tests pin the alias
// behaviour across engines and ensure it stays in lock-step with `lst`.

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

fn run_expect_fail(engine: &str, src: &str, entry: &str) -> (String, String) {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} unexpectedly succeeded for `{src}`: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

// ── Basic alias parity: lset behaves identically to lst on every engine. ────

const NUM_SRC: &str = "f>L n;lset [10,20,30] 1 99";

fn check_num(engine: &str) {
    assert_eq!(run(engine, NUM_SRC, "f"), "[10, 99, 30]", "engine={engine}");
}

#[test]
fn lset_num_tree() {
    check_num("--run-tree");
}

#[test]
fn lset_num_vm() {
    check_num("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn lset_num_cranelift() {
    check_num("--run-cranelift");
}

// Type variable: works on a list of text just like `lst`.
const TEXT_SRC: &str = "f>L t;lset [\"a\",\"b\",\"c\"] 2 \"X\"";

fn check_text(engine: &str) {
    assert_eq!(run(engine, TEXT_SRC, "f"), "[a, b, X]", "engine={engine}");
}

#[test]
fn lset_text_tree() {
    check_text("--run-tree");
}

#[test]
fn lset_text_vm() {
    check_text("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn lset_text_cranelift() {
    check_text("--run-cranelift");
}

// Boundary: first and last indices.
const FIRST_SRC: &str = "f>L n;lset [1,2,3] 0 9";
const LAST_SRC: &str = "f>L n;lset [1,2,3] 2 9";

#[test]
fn lset_first_tree() {
    assert_eq!(run("--run-tree", FIRST_SRC, "f"), "[9, 2, 3]");
}
#[test]
fn lset_first_vm() {
    assert_eq!(run("--run-vm", FIRST_SRC, "f"), "[9, 2, 3]");
}
#[test]
#[cfg(feature = "cranelift")]
fn lset_first_cranelift() {
    assert_eq!(run("--run-cranelift", FIRST_SRC, "f"), "[9, 2, 3]");
}

#[test]
fn lset_last_tree() {
    assert_eq!(run("--run-tree", LAST_SRC, "f"), "[1, 2, 9]");
}
#[test]
fn lset_last_vm() {
    assert_eq!(run("--run-vm", LAST_SRC, "f"), "[1, 2, 9]");
}
#[test]
#[cfg(feature = "cranelift")]
fn lset_last_cranelift() {
    assert_eq!(run("--run-cranelift", LAST_SRC, "f"), "[1, 2, 9]");
}

// Out-of-range index: tree/vm raise a runtime error; Cranelift returns the
// original list unchanged. Same split as the underlying `lst` builtin (see
// regression_list_mutation.rs).
const OOB_SRC: &str = "f>L n;lset [1,2,3] 5 99";

#[test]
fn lset_oob_tree_errors() {
    let (stdout, stderr) = run_expect_fail("--run-tree", OOB_SRC, "f");
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("ILO-R009") || combined.contains("ILO-R004"),
        "tree should raise R009/R004 on oob lset; got: {combined}"
    );
    assert!(
        combined.contains("out of range"),
        "tree error should mention 'out of range'; got: {combined}"
    );
}

#[test]
fn lset_oob_vm_errors() {
    let (stdout, stderr) = run_expect_fail("--run-vm", OOB_SRC, "f");
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("ILO-R004") || combined.contains("ILO-R009"),
        "vm should raise R004/R009 on oob lset; got: {combined}"
    );
    assert!(
        combined.contains("out of range"),
        "vm error should mention 'out of range'; got: {combined}"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn lset_oob_cranelift_passthrough() {
    // Cranelift JIT mirrors `lst`'s permissive behaviour: returns the list
    // unchanged rather than erroring. This is documented in the example
    // header and in regression_list_mutation.rs; the alias must match.
    assert_eq!(run("--run-cranelift", OOB_SRC, "f"), "[1, 2, 3]");
}

// Empty list with index 0: every engine treats this as out-of-range. Tree
// and VM error; Cranelift passes through.
const EMPTY_SRC: &str = "f>L n;xs=[];lset xs 0 1";

#[test]
fn lset_empty_tree_errors() {
    let (stdout, stderr) = run_expect_fail("--run-tree", EMPTY_SRC, "f");
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("out of range"),
        "tree should error on empty-list lset; got: {combined}"
    );
}

#[test]
fn lset_empty_vm_errors() {
    let (stdout, stderr) = run_expect_fail("--run-vm", EMPTY_SRC, "f");
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("out of range"),
        "vm should error on empty-list lset; got: {combined}"
    );
}

// Type mismatch: `lset` of a `L n` with a `t` value is rejected at verify
// time (ILO-T013) on every engine — verifier runs before any engine.
const TYPE_MISMATCH_SRC: &str = "f>L n;lset [1,2,3] 0 \"oops\"";

#[test]
fn lset_type_mismatch_rejected_tree() {
    let (stdout, stderr) = run_expect_fail("--run-tree", TYPE_MISMATCH_SRC, "f");
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("ILO-T013"),
        "expected verify error ILO-T013 on type mismatch; got: {combined}"
    );
}

#[test]
fn lset_type_mismatch_rejected_vm() {
    let (stdout, stderr) = run_expect_fail("--run-vm", TYPE_MISMATCH_SRC, "f");
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("ILO-T013"),
        "expected verify error ILO-T013 on type mismatch; got: {combined}"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn lset_type_mismatch_rejected_cranelift() {
    let (stdout, stderr) = run_expect_fail("--run-cranelift", TYPE_MISMATCH_SRC, "f");
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("ILO-T013"),
        "expected verify error ILO-T013 on type mismatch; got: {combined}"
    );
}

// Value-semantics: original list must not be mutated when lset is used.
// Mirrors lst_preserves_original from regression_list_mutation.rs. We bind
// the lset result to a real name (`ys`) so the parser doesn't mis-read a
// bare `_` as a call target, then probe the original list.
const PRESERVES_PROBE: &str = "f>n;xs=[1,2,3];ys=lset xs 1 99;at xs 1";

#[test]
fn lset_preserves_original_tree() {
    assert_eq!(run("--run-tree", PRESERVES_PROBE, "f"), "2");
}
#[test]
fn lset_preserves_original_vm() {
    assert_eq!(run("--run-vm", PRESERVES_PROBE, "f"), "2");
}
#[test]
#[cfg(feature = "cranelift")]
fn lset_preserves_original_cranelift() {
    assert_eq!(run("--run-cranelift", PRESERVES_PROBE, "f"), "2");
}

// Histogram pattern (the use-case the gis-analyst rerun needed). Uses lset
// in a foreach loop to in-place-rebuild bins. This is the example program
// shipped in examples/lset-alias.ilo, exercised here at the harness level
// across all three engines.
const HIST_SRC: &str = "hist samples:L n bins:L n>L n;@s samples{c=at bins s;bins=lset bins s +c 1};bins;\
     main>L n;hist [0,2,1,2,3,1,2,0] [0,0,0,0]";

#[test]
fn lset_histogram_tree() {
    assert_eq!(run("--run-tree", HIST_SRC, "main"), "[2, 2, 3, 1]");
}
#[test]
fn lset_histogram_vm() {
    assert_eq!(run("--run-vm", HIST_SRC, "main"), "[2, 2, 3, 1]");
}
#[test]
#[cfg(feature = "cranelift")]
fn lset_histogram_cranelift() {
    assert_eq!(run("--run-cranelift", HIST_SRC, "main"), "[2, 2, 3, 1]");
}

// Hint surfaces: when `lset` is used in JSON-mode CLI output, the canonical
// short-form hint must fire (precedent: `filter` -> `flt`).
#[test]
fn lset_emits_canonical_short_form_hint() {
    let out = ilo()
        .args([NUM_SRC, "--run-tree", "f"])
        .output()
        .expect("failed to run ilo");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("lset") && combined.contains("lst"),
        "expected `lset` -> `lst` hint somewhere in output; got: {combined}"
    );
}
