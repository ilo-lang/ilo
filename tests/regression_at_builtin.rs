// Regression tests for the `at xs i` builtin — i-th element of a list or text.
//
// Mirrors `hd` semantics: out-of-range is a runtime error (tree/vm) or TAG_NIL
// in cranelift JIT (which already returns nil for `hd` on empty list).

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

// Out-of-range: tree and vm engines raise a runtime error.
// Cranelift mirrors hd's JIT behaviour and returns nil (no error).
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
        stderr.contains("at") || stderr.contains("range") || stderr.contains("ILO-R009"),
        "engine={engine}: expected at/range/ILO-R009 error, got stderr={stderr}"
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

// Cranelift JIT mirrors hd's behaviour: returns nil on out-of-range.
#[test]
#[cfg(feature = "cranelift")]
fn at_out_of_range_cranelift() {
    let out = ilo()
        .args([OOR_SRC, "--run-cranelift", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "cranelift: expected success returning nil for at xs 99, got stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("nil"),
        "cranelift: expected stdout to contain nil, got {stdout}"
    );
}

// Negative index: tree/vm error, cranelift returns nil.
const NEG_SRC: &str = "f>n;xs=[10,20,30];at xs -1";

#[test]
fn at_negative_index_tree() {
    let out = ilo()
        .args([NEG_SRC, "--run-tree", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "tree: expected error for at xs -1, got stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("non-negative") || stderr.contains("ILO-R009"),
        "tree: expected non-negative/ILO-R009 error, got stderr={stderr}"
    );
}

#[test]
fn at_negative_index_vm() {
    let out = ilo()
        .args([NEG_SRC, "--run-vm", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "vm: expected error for at xs -1, got stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("non-negative") || stderr.contains("at"),
        "vm: expected non-negative/at error, got stderr={stderr}"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn at_negative_index_cranelift() {
    let out = ilo()
        .args([NEG_SRC, "--run-cranelift", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "cranelift: expected success returning nil for at xs -1, got stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("nil"),
        "cranelift: expected stdout to contain nil, got {stdout}"
    );
}
