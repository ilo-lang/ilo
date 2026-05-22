// Tests for hex (0xFF), binary (0b1010), and octal (0o755) numeric literals.
// These are converted to f64 at lex time and treated identically to decimal
// Number tokens by the parser and all evaluation engines.

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

// --- Hex literals ---

const HEX_FF: &str = "f>n;0xFF";

fn check_hex_ff(engine: &str) {
    assert_eq!(run(engine, HEX_FF, "f"), "255", "engine={engine}");
}

#[test]
fn hex_ff_vm() {
    check_hex_ff("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn hex_ff_jit() {
    check_hex_ff("--jit");
}

// Lowercase 0xff should equal uppercase 0xFF

const HEX_LOWER: &str = "f>n;0xff";

fn check_hex_lower(engine: &str) {
    assert_eq!(run(engine, HEX_LOWER, "f"), "255", "engine={engine}");
}

#[test]
fn hex_lower_vm() {
    check_hex_lower("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn hex_lower_jit() {
    check_hex_lower("--jit");
}

// 0x prefix with uppercase X

const HEX_UPPER_PREFIX: &str = "f>n;0XFF";

fn check_hex_upper_prefix(engine: &str) {
    assert_eq!(run(engine, HEX_UPPER_PREFIX, "f"), "255", "engine={engine}");
}

#[test]
fn hex_upper_prefix_vm() {
    check_hex_upper_prefix("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn hex_upper_prefix_jit() {
    check_hex_upper_prefix("--jit");
}

// Hex and decimal agree: 0xFF == 255

const HEX_EQ_DEC: &str = "f>b;==(0xFF) 255";

fn check_hex_eq_dec(engine: &str) {
    assert_eq!(run(engine, HEX_EQ_DEC, "f"), "true", "engine={engine}");
}

#[test]
fn hex_eq_dec_vm() {
    check_hex_eq_dec("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn hex_eq_dec_jit() {
    check_hex_eq_dec("--jit");
}

// --- Binary literals ---

const BIN_1010: &str = "f>n;0b1010";

fn check_bin_1010(engine: &str) {
    assert_eq!(run(engine, BIN_1010, "f"), "10", "engine={engine}");
}

#[test]
fn bin_1010_vm() {
    check_bin_1010("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn bin_1010_jit() {
    check_bin_1010("--jit");
}

// Uppercase 0B prefix

const BIN_UPPER_PREFIX: &str = "f>n;0B1010";

fn check_bin_upper_prefix(engine: &str) {
    assert_eq!(run(engine, BIN_UPPER_PREFIX, "f"), "10", "engine={engine}");
}

#[test]
fn bin_upper_prefix_vm() {
    check_bin_upper_prefix("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn bin_upper_prefix_jit() {
    check_bin_upper_prefix("--jit");
}

// Binary and decimal agree: 0b1010 == 10

const BIN_EQ_DEC: &str = "f>b;==(0b1010) 10";

fn check_bin_eq_dec(engine: &str) {
    assert_eq!(run(engine, BIN_EQ_DEC, "f"), "true", "engine={engine}");
}

#[test]
fn bin_eq_dec_vm() {
    check_bin_eq_dec("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn bin_eq_dec_jit() {
    check_bin_eq_dec("--jit");
}

// --- Octal literals ---

const OCT_755: &str = "f>n;0o755";

fn check_oct_755(engine: &str) {
    assert_eq!(run(engine, OCT_755, "f"), "493", "engine={engine}");
}

#[test]
fn oct_755_vm() {
    check_oct_755("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn oct_755_jit() {
    check_oct_755("--jit");
}

// Uppercase 0O prefix

const OCT_UPPER_PREFIX: &str = "f>n;0O755";

fn check_oct_upper_prefix(engine: &str) {
    assert_eq!(run(engine, OCT_UPPER_PREFIX, "f"), "493", "engine={engine}");
}

#[test]
fn oct_upper_prefix_vm() {
    check_oct_upper_prefix("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn oct_upper_prefix_jit() {
    check_oct_upper_prefix("--jit");
}

// Octal and decimal agree: 0o755 == 493

const OCT_EQ_DEC: &str = "f>b;==(0o755) 493";

fn check_oct_eq_dec(engine: &str) {
    assert_eq!(run(engine, OCT_EQ_DEC, "f"), "true", "engine={engine}");
}

#[test]
fn oct_eq_dec_vm() {
    check_oct_eq_dec("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn oct_eq_dec_jit() {
    check_oct_eq_dec("--jit");
}

// --- Arithmetic with radix literals ---

// Mixing radix literals in arithmetic: 0xFF + 0b1010 + 0o755 == 255 + 10 + 493 = 758

const MIXED_ARITH: &str = "f>n;+(+(0xFF) 0b1010) 0o755";

fn check_mixed_arith(engine: &str) {
    assert_eq!(run(engine, MIXED_ARITH, "f"), "758", "engine={engine}");
}

#[test]
fn mixed_arith_vm() {
    check_mixed_arith("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn mixed_arith_jit() {
    check_mixed_arith("--jit");
}

// --- Error: bare prefix with no digits ---
// `0x` with no following hex digits should produce a parse/lex error,
// not silently emit 0. We check that the process exits non-zero.

#[test]
fn hex_empty_digits_is_error() {
    let out = ilo()
        .args(["f>n;0x", "--vm", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected failure for bare `0x`, but ilo exited successfully"
    );
}

#[test]
fn bin_empty_digits_is_error() {
    let out = ilo()
        .args(["f>n;0b", "--vm", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected failure for bare `0b`, but ilo exited successfully"
    );
}

#[test]
fn oct_empty_digits_is_error() {
    let out = ilo()
        .args(["f>n;0o", "--vm", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected failure for bare `0o`, but ilo exited successfully"
    );
}
