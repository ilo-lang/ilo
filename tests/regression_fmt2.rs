// Regression tests for the `fmt2 x digits` builtin — format a number to `digits`
// decimal places, returning text. Uses Rust's standard half-to-even rounding
// (the default for `format!("{:.N$}")`).
//
// Cross-engine: tree, vm, and (when enabled) cranelift JIT.

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

// Basic: 3.14159 to 2 decimals → "3.14".
const BASIC_SRC: &str = "f>t;fmt2 3.14159 2";

fn check_basic(engine: &str) {
    assert_eq!(run(engine, BASIC_SRC, "f"), "3.14", "engine={engine}");
}

#[test]
fn fmt2_basic_tree() {
    check_basic("--run-tree");
}

#[test]
fn fmt2_basic_vm() {
    check_basic("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn fmt2_basic_cranelift() {
    check_basic("--run-cranelift");
}

// Zero decimals: integer-valued float prints without a fractional part.
const ZERO_DIGITS_SRC: &str = "f>t;fmt2 1.0 0";

fn check_zero_digits(engine: &str) {
    assert_eq!(run(engine, ZERO_DIGITS_SRC, "f"), "1", "engine={engine}");
}

#[test]
fn fmt2_zero_digits_tree() {
    check_zero_digits("--run-tree");
}

#[test]
fn fmt2_zero_digits_vm() {
    check_zero_digits("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn fmt2_zero_digits_cranelift() {
    check_zero_digits("--run-cranelift");
}

// Long fractional number, asymmetric truncation.
const LONG_FRAC_SRC: &str = "f>t;fmt2 0.85025037 4";

fn check_long_frac(engine: &str) {
    assert_eq!(run(engine, LONG_FRAC_SRC, "f"), "0.8503", "engine={engine}");
}

#[test]
fn fmt2_long_frac_tree() {
    check_long_frac("--run-tree");
}

#[test]
fn fmt2_long_frac_vm() {
    check_long_frac("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn fmt2_long_frac_cranelift() {
    check_long_frac("--run-cranelift");
}

// Half-to-even (banker's rounding): 1.5 → "2", 2.5 → "2".
const HALF_EVEN_UP_SRC: &str = "f>t;fmt2 1.5 0";

fn check_half_even_up(engine: &str) {
    assert_eq!(run(engine, HALF_EVEN_UP_SRC, "f"), "2", "engine={engine}");
}

#[test]
fn fmt2_half_even_up_tree() {
    check_half_even_up("--run-tree");
}

#[test]
fn fmt2_half_even_up_vm() {
    check_half_even_up("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn fmt2_half_even_up_cranelift() {
    check_half_even_up("--run-cranelift");
}

const HALF_EVEN_DOWN_SRC: &str = "f>t;fmt2 2.5 0";

fn check_half_even_down(engine: &str) {
    assert_eq!(run(engine, HALF_EVEN_DOWN_SRC, "f"), "2", "engine={engine}");
}

#[test]
fn fmt2_half_even_down_tree() {
    check_half_even_down("--run-tree");
}

#[test]
fn fmt2_half_even_down_vm() {
    check_half_even_down("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn fmt2_half_even_down_cranelift() {
    check_half_even_down("--run-cranelift");
}

// Negative digits clamp to 0 (integer formatting). Use a literal -1.
const NEG_DIGITS_SRC: &str = "f>t;fmt2 3.7 -1";

fn check_neg_digits(engine: &str) {
    assert_eq!(run(engine, NEG_DIGITS_SRC, "f"), "4", "engine={engine}");
}

#[test]
fn fmt2_neg_digits_tree() {
    check_neg_digits("--run-tree");
}

#[test]
fn fmt2_neg_digits_vm() {
    check_neg_digits("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn fmt2_neg_digits_cranelift() {
    check_neg_digits("--run-cranelift");
}
