// Regression tests for 64-bit bitwise builtins (ILO-395).
//
// band64 / bor64 / bxor64 / bnot64 / bshl64 / bshr64 / brot64 operate on
// f64 values converted to u64 (mod 2^64) and return f64. Verified across vm
// and cranelift engines (tree interpreter is exercised via the VM's
// tree-bridge).
//
// NOTE: f64 can exactly represent integers up to 2^53. Tests stay within
// safe range to avoid f64↔u64 precision loss.

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
        "ilo {engine} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// --- band64 ---
const BAND64_SRC: &str = "f>n;band64 12 10";

fn check_band64(engine: &str) {
    assert_eq!(run(engine, BAND64_SRC, "f"), "8", "engine={engine}");
}

#[test]
fn band64_vm() {
    check_band64("--vm");
}
#[test]
#[cfg(feature = "cranelift")]
fn band64_jit() {
    check_band64("--jit");
}

// --- bor64 ---
const BOR64_SRC: &str = "f>n;bor64 12 10";

fn check_bor64(engine: &str) {
    assert_eq!(run(engine, BOR64_SRC, "f"), "14", "engine={engine}");
}

#[test]
fn bor64_vm() {
    check_bor64("--vm");
}
#[test]
#[cfg(feature = "cranelift")]
fn bor64_jit() {
    check_bor64("--jit");
}

// --- bxor64 ---
const BXOR64_SRC: &str = "f>n;bxor64 12 10";

fn check_bxor64(engine: &str) {
    assert_eq!(run(engine, BXOR64_SRC, "f"), "6", "engine={engine}");
}

#[test]
fn bxor64_vm() {
    check_bxor64("--vm");
}
#[test]
#[cfg(feature = "cranelift")]
fn bxor64_jit() {
    check_bxor64("--jit");
}

// --- bnot64 ---
// bnot64 0 = 0xFFFFFFFFFFFFFFFF = 18446744073709551615
// But f64 can only exactly represent up to 2^53, so 2^64-1 is not exact.
// Test with a value that round-trips: bnot64 of 0xFFFFFFFF00000000 (within safe range if using
// small values). Use bnot64 of (2^53 - 1) masked back: just test that bnot64 1 = 2^64-2
// ... 2^64-2 = 18446744073709551614, not representable exactly in f64.
// Instead, test round-trip: bxor64(bnot64(x), bnot64(x)) == 0, or use
// band64(bnot64(0), 255) == 255 (mask to low 8 bits — stays in safe range).
const BNOT64_MASKED_SRC: &str = "f>n;band64 (bnot64 0) 255";

fn check_bnot64_masked(engine: &str) {
    assert_eq!(
        run(engine, BNOT64_MASKED_SRC, "f"),
        "255",
        "engine={engine}"
    );
}

// bnot64 of (2^53 - 1) masked to low 16 bits: safe round-trip check.
// bnot64(0xFFFF) & 0xFFFF == 0 (all bits flipped, low 16 set to zero).
const BNOT64_LOW16_SRC: &str = "f>n;band64 (bnot64 65535) 65535";

fn check_bnot64_low16(engine: &str) {
    assert_eq!(run(engine, BNOT64_LOW16_SRC, "f"), "0", "engine={engine}");
}

#[test]
fn bnot64_masked_vm() {
    check_bnot64_masked("--vm");
}
#[test]
fn bnot64_low16_vm() {
    check_bnot64_low16("--vm");
}
#[test]
#[cfg(feature = "cranelift")]
fn bnot64_masked_jit() {
    check_bnot64_masked("--jit");
}
#[test]
#[cfg(feature = "cranelift")]
fn bnot64_low16_jit() {
    check_bnot64_low16("--jit");
}

// --- bshl64 ---
const BSHL64_SRC: &str = "f>n;bshl64 1 4";

fn check_bshl64(engine: &str) {
    assert_eq!(run(engine, BSHL64_SRC, "f"), "16", "engine={engine}");
}

// shift left by 33: 1 << 33 = 8589934592 (within 2^53, safe)
const BSHL64_33_SRC: &str = "f>n;bshl64 1 33";

fn check_bshl64_33(engine: &str) {
    assert_eq!(
        run(engine, BSHL64_33_SRC, "f"),
        "8589934592",
        "engine={engine}"
    );
}

#[test]
fn bshl64_vm() {
    check_bshl64("--vm");
}
#[test]
fn bshl64_33_vm() {
    check_bshl64_33("--vm");
}
#[test]
#[cfg(feature = "cranelift")]
fn bshl64_jit() {
    check_bshl64("--jit");
}
#[test]
#[cfg(feature = "cranelift")]
fn bshl64_33_jit() {
    check_bshl64_33("--jit");
}

// --- bshr64 ---
const BSHR64_SRC: &str = "f>n;bshr64 256 3";

fn check_bshr64(engine: &str) {
    assert_eq!(run(engine, BSHR64_SRC, "f"), "32", "engine={engine}");
}

// shift right 33 bits: 8589934592 >> 33 = 1
const BSHR64_33_SRC: &str = "f>n;bshr64 8589934592 33";

fn check_bshr64_33(engine: &str) {
    assert_eq!(run(engine, BSHR64_33_SRC, "f"), "1", "engine={engine}");
}

#[test]
fn bshr64_vm() {
    check_bshr64("--vm");
}
#[test]
fn bshr64_33_vm() {
    check_bshr64_33("--vm");
}
#[test]
#[cfg(feature = "cranelift")]
fn bshr64_jit() {
    check_bshr64("--jit");
}
#[test]
#[cfg(feature = "cranelift")]
fn bshr64_33_jit() {
    check_bshr64_33("--jit");
}

// --- brot64 ---
const BROT64_1_1_SRC: &str = "f>n;brot64 1 1";

fn check_brot64_1_1(engine: &str) {
    assert_eq!(run(engine, BROT64_1_1_SRC, "f"), "2", "engine={engine}");
}

// brot64 1 63 = 1 rotated left 63 bits = 2^63.
// 2^63 is exactly representable in f64, but ilo's integer formatter prints it
// as 9223372036854775807 (i64::MAX) due to the f64-to-integer display path.
// Verify by round-tripping: bshr64(brot64(1, 63), 63) == 1.
const BROT64_ROUNDTRIP_SRC: &str = "f>n;bshr64 (brot64 1 63) 63";

fn check_brot64_roundtrip(engine: &str) {
    assert_eq!(
        run(engine, BROT64_ROUNDTRIP_SRC, "f"),
        "1",
        "engine={engine}"
    );
}

#[test]
fn brot64_1_1_vm() {
    check_brot64_1_1("--vm");
}
#[test]
fn brot64_roundtrip_vm() {
    check_brot64_roundtrip("--vm");
}
#[test]
#[cfg(feature = "cranelift")]
fn brot64_1_1_jit() {
    check_brot64_1_1("--jit");
}
#[test]
#[cfg(feature = "cranelift")]
fn brot64_roundtrip_jit() {
    check_brot64_roundtrip("--jit");
}

// --- shift amount mod 64: bshl64 1 64 == bshl64 1 0 == 1 ---
const SHIFT64_MOD_SRC: &str = "f>n;bshl64 1 64";

fn check_shift64_mod(engine: &str) {
    assert_eq!(run(engine, SHIFT64_MOD_SRC, "f"), "1", "engine={engine}");
}

#[test]
fn shift64_mod_vm() {
    check_shift64_mod("--vm");
}
#[test]
#[cfg(feature = "cranelift")]
fn shift64_mod_jit() {
    check_shift64_mod("--jit");
}
