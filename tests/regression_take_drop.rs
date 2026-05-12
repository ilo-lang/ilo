// Regression tests for the `take n xs` and `drop n xs` builtins.
//
// take n xs — first n elements of a list/text (truncates on out-of-range).
// drop n xs — list/text with first n elements removed (truncates).
// Negative n is a runtime error (tree/vm); cranelift JIT returns nil.

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

// ── take: basic ────────────────────────────────────────────────────────
const TAKE_BASIC: &str = "f>L n;xs=[1,2,3,4,5];take 2 xs";

fn check_take_basic(engine: &str) {
    assert_eq!(run(engine, TAKE_BASIC, "f"), "[1, 2]", "engine={engine}");
}

#[test]
fn take_basic_tree() {
    check_take_basic("--run-tree");
}

#[test]
fn take_basic_vm() {
    check_take_basic("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn take_basic_cranelift() {
    check_take_basic("--run-cranelift");
}

// ── take: truncate on out-of-range ────────────────────────────────────
const TAKE_TRUNC: &str = "f>L n;xs=[1,2,3];take 5 xs";

fn check_take_trunc(engine: &str) {
    assert_eq!(run(engine, TAKE_TRUNC, "f"), "[1, 2, 3]", "engine={engine}");
}

#[test]
fn take_trunc_tree() {
    check_take_trunc("--run-tree");
}

#[test]
fn take_trunc_vm() {
    check_take_trunc("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn take_trunc_cranelift() {
    check_take_trunc("--run-cranelift");
}

// ── take: empty result ────────────────────────────────────────────────
const TAKE_ZERO: &str = "f>L n;xs=[1,2,3];take 0 xs";

fn check_take_zero(engine: &str) {
    assert_eq!(run(engine, TAKE_ZERO, "f"), "[]", "engine={engine}");
}

#[test]
fn take_zero_tree() {
    check_take_zero("--run-tree");
}

#[test]
fn take_zero_vm() {
    check_take_zero("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn take_zero_cranelift() {
    check_take_zero("--run-cranelift");
}

// ── drop: basic ───────────────────────────────────────────────────────
const DROP_BASIC: &str = "f>L n;xs=[1,2,3,4,5];drop 2 xs";

fn check_drop_basic(engine: &str) {
    assert_eq!(run(engine, DROP_BASIC, "f"), "[3, 4, 5]", "engine={engine}");
}

#[test]
fn drop_basic_tree() {
    check_drop_basic("--run-tree");
}

#[test]
fn drop_basic_vm() {
    check_drop_basic("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn drop_basic_cranelift() {
    check_drop_basic("--run-cranelift");
}

// ── drop: truncate (n > len) returns empty ────────────────────────────
const DROP_TRUNC: &str = "f>L n;xs=[1,2,3];drop 5 xs";

fn check_drop_trunc(engine: &str) {
    assert_eq!(run(engine, DROP_TRUNC, "f"), "[]", "engine={engine}");
}

#[test]
fn drop_trunc_tree() {
    check_drop_trunc("--run-tree");
}

#[test]
fn drop_trunc_vm() {
    check_drop_trunc("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn drop_trunc_cranelift() {
    check_drop_trunc("--run-cranelift");
}

// ── empty input ───────────────────────────────────────────────────────
const TAKE_EMPTY: &str = "f>L n;xs=[];take 3 xs";

fn check_take_empty(engine: &str) {
    assert_eq!(run(engine, TAKE_EMPTY, "f"), "[]", "engine={engine}");
}

#[test]
fn take_empty_tree() {
    check_take_empty("--run-tree");
}

#[test]
fn take_empty_vm() {
    check_take_empty("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn take_empty_cranelift() {
    check_take_empty("--run-cranelift");
}

// ── type variable: take on text ────────────────────────────────────────
const TAKE_TEXT: &str = "f>t;take 3 \"hello\"";

fn check_take_text(engine: &str) {
    assert_eq!(run(engine, TAKE_TEXT, "f"), "hel", "engine={engine}");
}

#[test]
fn take_text_tree() {
    check_take_text("--run-tree");
}

#[test]
fn take_text_vm() {
    check_take_text("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn take_text_cranelift() {
    check_take_text("--run-cranelift");
}

// ── drop on text ──────────────────────────────────────────────────────
const DROP_TEXT: &str = "f>t;drop 2 \"hello\"";

fn check_drop_text(engine: &str) {
    assert_eq!(run(engine, DROP_TEXT, "f"), "llo", "engine={engine}");
}

#[test]
fn drop_text_tree() {
    check_drop_text("--run-tree");
}

#[test]
fn drop_text_vm() {
    check_drop_text("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn drop_text_cranelift() {
    check_drop_text("--run-cranelift");
}

// ── negative count: error on tree/vm; nil on cranelift JIT ────────────
const TAKE_NEG: &str = "f>L n;xs=[1,2,3];take -1 xs";

fn check_take_neg_error(engine: &str) {
    let out = ilo()
        .args([TAKE_NEG, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "engine={engine}: expected error for take -1, got stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("take") || stderr.contains("non-negative") || stderr.contains("ILO-R009"),
        "engine={engine}: expected take/non-negative error, got stderr={stderr}"
    );
}

#[test]
fn take_negative_tree() {
    check_take_neg_error("--run-tree");
}

#[test]
fn take_negative_vm() {
    check_take_neg_error("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn take_negative_cranelift() {
    // Cranelift JIT mirrors at/hd: returns nil on guard failure rather than erroring.
    let out = ilo()
        .args([TAKE_NEG, "--run-cranelift", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "cranelift: expected success returning nil for take -1, got stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("nil"),
        "cranelift: expected nil, got {stdout}"
    );
}

const DROP_NEG: &str = "f>L n;xs=[1,2,3];drop -1 xs";

fn check_drop_neg_error(engine: &str) {
    let out = ilo()
        .args([DROP_NEG, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "engine={engine}: expected error for drop -1, got stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("drop") || stderr.contains("non-negative") || stderr.contains("ILO-R009"),
        "engine={engine}: expected drop/non-negative error, got stderr={stderr}"
    );
}

#[test]
fn drop_negative_tree() {
    check_drop_neg_error("--run-tree");
}

#[test]
fn drop_negative_vm() {
    check_drop_neg_error("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn drop_negative_cranelift() {
    let out = ilo()
        .args([DROP_NEG, "--run-cranelift", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "cranelift: expected success returning nil for drop -1, got stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("nil"),
        "cranelift: expected nil, got {stdout}"
    );
}

// ── type variable: take/drop preserve element type (list of text) ─────
const TAKE_TEXT_LIST: &str = "f>L t;xs=[\"a\",\"b\",\"c\",\"d\"];take 2 xs";

fn check_take_text_list(engine: &str) {
    assert_eq!(
        run(engine, TAKE_TEXT_LIST, "f"),
        "[a, b]",
        "engine={engine}"
    );
}

#[test]
fn take_text_list_tree() {
    check_take_text_list("--run-tree");
}

#[test]
fn take_text_list_vm() {
    check_take_text_list("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn take_text_list_cranelift() {
    check_take_text_list("--run-cranelift");
}
