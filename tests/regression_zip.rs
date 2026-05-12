// Regression tests for the `zip xs ys` builtin — pair-wise combine two lists.
//
// Returns L (L a) of 2-element pairs. Truncates to the shorter input
// (Python convention). Verified across tree, vm, and cranelift engines.

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

// Basic: zip two same-length number lists.
const BASIC_SRC: &str = "f>L (L n);zip [1,2,3] [10,20,30]";

fn check_basic(engine: &str) {
    assert_eq!(
        run(engine, BASIC_SRC, "f"),
        "[[1, 10], [2, 20], [3, 30]]",
        "engine={engine}"
    );
}

#[test]
fn zip_basic_tree() {
    check_basic("--run-tree");
}

#[test]
fn zip_basic_vm() {
    check_basic("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn zip_basic_cranelift() {
    check_basic("--run-cranelift");
}

// Truncate to shorter (xs longer than ys).
const TRUNC_LONG_XS_SRC: &str = "f>L (L n);zip [1,2,3,4] [10,20]";

fn check_trunc_long_xs(engine: &str) {
    assert_eq!(
        run(engine, TRUNC_LONG_XS_SRC, "f"),
        "[[1, 10], [2, 20]]",
        "engine={engine}"
    );
}

#[test]
fn zip_trunc_long_xs_tree() {
    check_trunc_long_xs("--run-tree");
}

#[test]
fn zip_trunc_long_xs_vm() {
    check_trunc_long_xs("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn zip_trunc_long_xs_cranelift() {
    check_trunc_long_xs("--run-cranelift");
}

// Truncate to shorter (ys longer than xs).
const TRUNC_LONG_YS_SRC: &str = "f>L (L n);zip [1,2] [10,20,30,40]";

fn check_trunc_long_ys(engine: &str) {
    assert_eq!(
        run(engine, TRUNC_LONG_YS_SRC, "f"),
        "[[1, 10], [2, 20]]",
        "engine={engine}"
    );
}

#[test]
fn zip_trunc_long_ys_tree() {
    check_trunc_long_ys("--run-tree");
}

#[test]
fn zip_trunc_long_ys_vm() {
    check_trunc_long_ys("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn zip_trunc_long_ys_cranelift() {
    check_trunc_long_ys("--run-cranelift");
}

// Empty list: either side empty yields empty.
const EMPTY_SRC: &str = "f>L (L n);zip [] [1,2,3]";

fn check_empty(engine: &str) {
    assert_eq!(run(engine, EMPTY_SRC, "f"), "[]", "engine={engine}");
}

#[test]
fn zip_empty_tree() {
    check_empty("--run-tree");
}

#[test]
fn zip_empty_vm() {
    check_empty("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn zip_empty_cranelift() {
    check_empty("--run-cranelift");
}

// Mixed types via type variable: zip text with numbers.
const MIXED_SRC: &str = "f>L (L a);zip [\"a\",\"b\"] [1,2]";

fn check_mixed(engine: &str) {
    assert_eq!(
        run(engine, MIXED_SRC, "f"),
        "[[a, 1], [b, 2]]",
        "engine={engine}"
    );
}

#[test]
fn zip_mixed_tree() {
    check_mixed("--run-tree");
}

#[test]
fn zip_mixed_vm() {
    check_mixed("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn zip_mixed_cranelift() {
    check_mixed("--run-cranelift");
}
