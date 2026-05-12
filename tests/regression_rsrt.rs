// Regression tests for the `rsrt xs` builtin — descending sort of a list.
//
// `rsrt` mirrors `srt` but in reverse order. Numeric lists sort descending,
// text lists sort lexicographically descending. Same `Ord` impl as `srt`,
// just the inverse comparator.

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

// Numeric list — manifest example from the spec.
const NUM_SRC: &str = "f>L n;xs=[3,1,4,1,5,9,2,6];rsrt xs";

fn check_nums(engine: &str) {
    assert_eq!(
        run(engine, NUM_SRC, "f"),
        "[9, 6, 5, 4, 3, 2, 1, 1]",
        "engine={engine}"
    );
}

#[test]
fn rsrt_nums_tree() {
    check_nums("--run-tree");
}

#[test]
fn rsrt_nums_vm() {
    check_nums("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn rsrt_nums_cranelift() {
    check_nums("--run-cranelift");
}

// String list — lexicographic descending.
const TEXT_SRC: &str = "f>L t;xs=[\"banana\",\"apple\",\"cherry\"];rsrt xs";

fn check_text(engine: &str) {
    assert_eq!(
        run(engine, TEXT_SRC, "f"),
        "[cherry, banana, apple]",
        "engine={engine}"
    );
}

#[test]
fn rsrt_text_tree() {
    check_text("--run-tree");
}

#[test]
fn rsrt_text_vm() {
    check_text("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn rsrt_text_cranelift() {
    check_text("--run-cranelift");
}

// Empty list — round-trips as empty.
const EMPTY_SRC: &str = "f>L n;rsrt []";

fn check_empty(engine: &str) {
    assert_eq!(run(engine, EMPTY_SRC, "f"), "[]", "engine={engine}");
}

#[test]
fn rsrt_empty_tree() {
    check_empty("--run-tree");
}

#[test]
fn rsrt_empty_vm() {
    check_empty("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn rsrt_empty_cranelift() {
    check_empty("--run-cranelift");
}

// Single element — round-trips unchanged. Use a two-element collapse
// via slc so the parser doesn't mistake `[42]` for subscripting.
const SINGLE_SRC: &str = "f>L n;xs=[42,99];rsrt (slc xs 0 1)";

fn check_single(engine: &str) {
    assert_eq!(run(engine, SINGLE_SRC, "f"), "[42]", "engine={engine}");
}

#[test]
fn rsrt_single_tree() {
    check_single("--run-tree");
}

#[test]
fn rsrt_single_vm() {
    check_single("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn rsrt_single_cranelift() {
    check_single("--run-cranelift");
}

// Type-variable signature: rsrt :: L a > L a — accepts any list element type.
// Exercises the polymorphic shape by routing through a wrapper fn.
const TYPEVAR_SRC: &str = "g xs:L a>L a;rsrt xs   f>L n;g [3,1,2]";

fn check_typevar(engine: &str) {
    assert_eq!(
        run(engine, TYPEVAR_SRC, "f"),
        "[3, 2, 1]",
        "engine={engine}"
    );
}

#[test]
fn rsrt_typevar_tree() {
    check_typevar("--run-tree");
}

#[test]
fn rsrt_typevar_vm() {
    check_typevar("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn rsrt_typevar_cranelift() {
    check_typevar("--run-cranelift");
}

// Text input — mirror `srt` but in reverse. Sorts characters by codepoint
// descending. Symmetry with `srt` is the whole point of this branch.
const STR_ASC_SRC: &str = "f>t;rsrt \"abc\"";

fn check_str_asc(engine: &str) {
    assert_eq!(run(engine, STR_ASC_SRC, "f"), "cba", "engine={engine}");
}

#[test]
fn rsrt_str_asc_tree() {
    check_str_asc("--run-tree");
}

#[test]
fn rsrt_str_asc_vm() {
    check_str_asc("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn rsrt_str_asc_cranelift() {
    check_str_asc("--run-cranelift");
}

// Already-descending text — round-trips unchanged.
const STR_DESC_SRC: &str = "f>t;rsrt \"cba\"";

fn check_str_desc(engine: &str) {
    assert_eq!(run(engine, STR_DESC_SRC, "f"), "cba", "engine={engine}");
}

#[test]
fn rsrt_str_desc_tree() {
    check_str_desc("--run-tree");
}

#[test]
fn rsrt_str_desc_vm() {
    check_str_desc("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn rsrt_str_desc_cranelift() {
    check_str_desc("--run-cranelift");
}
