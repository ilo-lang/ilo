// Regression tests for the `enumerate xs` builtin — pair each element of a
// list with its index, returning a list of [index, value] pairs.
//
// Returns L (L _): inner pair holds a number (index) and an `a` (element),
// so the element type is erased to `_`. Verified across tree, vm, and
// cranelift engines.

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

// Basic: enumerate a list of strings.
const STRINGS_SRC: &str = "f>L (L _);enumerate [\"a\",\"b\",\"c\"]";

fn check_strings(engine: &str) {
    assert_eq!(
        run(engine, STRINGS_SRC, "f"),
        "[[0, a], [1, b], [2, c]]",
        "engine={engine}"
    );
}

#[test]
fn enumerate_strings_tree() {
    check_strings("--run-tree");
}

#[test]
fn enumerate_strings_vm() {
    check_strings("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn enumerate_strings_cranelift() {
    check_strings("--run-cranelift");
}

// Empty list: produces an empty list.
const EMPTY_SRC: &str = "f>L (L _);enumerate []";

fn check_empty(engine: &str) {
    assert_eq!(run(engine, EMPTY_SRC, "f"), "[]", "engine={engine}");
}

#[test]
fn enumerate_empty_tree() {
    check_empty("--run-tree");
}

#[test]
fn enumerate_empty_vm() {
    check_empty("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn enumerate_empty_cranelift() {
    check_empty("--run-cranelift");
}

// Singleton: a single-element list.
const SINGLE_SRC: &str = "f>L (L _);enumerate [10]";

fn check_single(engine: &str) {
    assert_eq!(run(engine, SINGLE_SRC, "f"), "[[0, 10]]", "engine={engine}");
}

#[test]
fn enumerate_single_tree() {
    check_single("--run-tree");
}

#[test]
fn enumerate_single_vm() {
    check_single("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn enumerate_single_cranelift() {
    check_single("--run-cranelift");
}

// Type-variable: numbers work too (element type erased to `_`).
const NUMBERS_SRC: &str = "f>L (L _);enumerate [100,200,300]";

fn check_numbers(engine: &str) {
    assert_eq!(
        run(engine, NUMBERS_SRC, "f"),
        "[[0, 100], [1, 200], [2, 300]]",
        "engine={engine}"
    );
}

#[test]
fn enumerate_numbers_tree() {
    check_numbers("--run-tree");
}

#[test]
fn enumerate_numbers_vm() {
    check_numbers("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn enumerate_numbers_cranelift() {
    check_numbers("--run-cranelift");
}

// Use case: pass the result through `hd` to grab the first [i,v] pair.
const FIRST_PAIR_SRC: &str = "f>L _;hd (enumerate [\"x\",\"y\",\"z\"])";

fn check_first_pair(engine: &str) {
    assert_eq!(
        run(engine, FIRST_PAIR_SRC, "f"),
        "[0, x]",
        "engine={engine}"
    );
}

#[test]
fn enumerate_first_pair_tree() {
    check_first_pair("--run-tree");
}

#[test]
fn enumerate_first_pair_vm() {
    check_first_pair("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn enumerate_first_pair_cranelift() {
    check_first_pair("--run-cranelift");
}
