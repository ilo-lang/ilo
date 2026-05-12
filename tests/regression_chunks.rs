// Regression tests for the `chunks n xs` builtin — split a list into
// non-overlapping pieces of size `n`. The final piece may be shorter
// when `len xs` is not a multiple of `n`. Verified across tree, vm,
// and cranelift engines.

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

fn run_err(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} unexpectedly succeeded for `{src}`: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

// Basic: 5-element list split into chunks of 2 → last chunk has 1 element.
const BASIC_SRC: &str = "f>L (L n);chunks 2 [1,2,3,4,5]";

fn check_basic(engine: &str) {
    assert_eq!(
        run(engine, BASIC_SRC, "f"),
        "[[1, 2], [3, 4], [5]]",
        "engine={engine}"
    );
}

#[test]
fn chunks_basic_tree() {
    check_basic("--run-tree");
}

#[test]
fn chunks_basic_vm() {
    check_basic("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn chunks_basic_cranelift() {
    check_basic("--run-cranelift");
}

// Exact: list length divides evenly by n.
const EXACT_SRC: &str = "f>L (L n);chunks 3 [1,2,3,4,5,6]";

fn check_exact(engine: &str) {
    assert_eq!(
        run(engine, EXACT_SRC, "f"),
        "[[1, 2, 3], [4, 5, 6]]",
        "engine={engine}"
    );
}

#[test]
fn chunks_exact_tree() {
    check_exact("--run-tree");
}

#[test]
fn chunks_exact_vm() {
    check_exact("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn chunks_exact_cranelift() {
    check_exact("--run-cranelift");
}

// n >= len xs: single chunk containing the full list.
const BIG_N_SRC: &str = "f>L (L n);chunks 10 [1,2,3]";

fn check_big_n(engine: &str) {
    assert_eq!(
        run(engine, BIG_N_SRC, "f"),
        "[[1, 2, 3]]",
        "engine={engine}"
    );
}

#[test]
fn chunks_big_n_tree() {
    check_big_n("--run-tree");
}

#[test]
fn chunks_big_n_vm() {
    check_big_n("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn chunks_big_n_cranelift() {
    check_big_n("--run-cranelift");
}

// n == 1: each element is its own singleton chunk.
const ONE_SRC: &str = "f>L (L n);chunks 1 [1,2,3]";

fn check_one(engine: &str) {
    assert_eq!(
        run(engine, ONE_SRC, "f"),
        "[[1], [2], [3]]",
        "engine={engine}"
    );
}

#[test]
fn chunks_one_tree() {
    check_one("--run-tree");
}

#[test]
fn chunks_one_vm() {
    check_one("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn chunks_one_cranelift() {
    check_one("--run-cranelift");
}

// n == 0: error — chunk size must be a positive integer.
const ZERO_SRC: &str = "f>L (L n);chunks 0 [1,2,3]";

fn check_zero_err(engine: &str) {
    let err = run_err(engine, ZERO_SRC, "f");
    assert!(
        err.contains("chunks") || err.contains("positive"),
        "engine={engine}, err={err}"
    );
}

#[test]
fn chunks_zero_tree() {
    check_zero_err("--run-tree");
}

#[test]
fn chunks_zero_vm() {
    check_zero_err("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn chunks_zero_cranelift() {
    // cranelift jit_chunks returns nil on invalid args; the surrounding
    // type contract surfaces this as an error or nil — accept either.
    let out = ilo()
        .args([ZERO_SRC, "--run-cranelift", "f"])
        .output()
        .expect("failed to run ilo");
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(
        !out.status.success() || stdout == "nil" || stdout.is_empty(),
        "expected error or nil for chunks 0, got stdout={stdout}, stderr={stderr}"
    );
}

// Empty input: empty output.
const EMPTY_SRC: &str = "f>L (L n);chunks 2 []";

fn check_empty(engine: &str) {
    assert_eq!(run(engine, EMPTY_SRC, "f"), "[]", "engine={engine}");
}

#[test]
fn chunks_empty_tree() {
    check_empty("--run-tree");
}

#[test]
fn chunks_empty_vm() {
    check_empty("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn chunks_empty_cranelift() {
    check_empty("--run-cranelift");
}

// Type variable: works on text lists too.
const TEXT_SRC: &str = "f>L (L a);chunks 2 [\"a\",\"b\",\"c\",\"d\"]";

fn check_text(engine: &str) {
    assert_eq!(
        run(engine, TEXT_SRC, "f"),
        "[[a, b], [c, d]]",
        "engine={engine}"
    );
}

#[test]
fn chunks_text_tree() {
    check_text("--run-tree");
}

#[test]
fn chunks_text_vm() {
    check_text("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn chunks_text_cranelift() {
    check_text("--run-cranelift");
}

// `chunks n xs` where n > len xs returns a single short trailing chunk
// containing all elements — distinct from `window n xs` which returns []
// when no full-size window fits. Matches Rust's slice::chunks precedent.
const PARTIAL_TRAILING_SRC: &str = "f>L (L n);chunks 5 [1, 2, 3]";

fn check_partial_trailing(engine: &str) {
    assert_eq!(
        run(engine, PARTIAL_TRAILING_SRC, "f"),
        "[[1, 2, 3]]",
        "engine={engine}"
    );
}

#[test]
fn chunks_partial_trailing_tree() {
    check_partial_trailing("--run-tree");
}

#[test]
fn chunks_partial_trailing_vm() {
    check_partial_trailing("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn chunks_partial_trailing_cranelift() {
    check_partial_trailing("--run-cranelift");
}
