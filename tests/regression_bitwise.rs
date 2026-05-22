// Regression tests for bitwise builtins (ILO-58 MVP).
//
// band / bor / bxor / bnot / bshl / bshr / brot all operate on f64 values
// converted to u32 (mod 2^32) and return f64. Verified across vm and
// cranelift engines (tree interpreter is exercised via the VM's tree-bridge).

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

// --- band ---
const BAND_SRC: &str = "f>n;band 12 10";

fn check_band(engine: &str) {
    assert_eq!(run(engine, BAND_SRC, "f"), "8", "engine={engine}");
}

#[test]
fn band_vm() { check_band("--vm"); }
#[test]
#[cfg(feature = "cranelift")]
fn band_jit() { check_band("--jit"); }

// --- bor ---
const BOR_SRC: &str = "f>n;bor 12 10";

fn check_bor(engine: &str) {
    assert_eq!(run(engine, BOR_SRC, "f"), "14", "engine={engine}");
}

#[test]
fn bor_vm() { check_bor("--vm"); }
#[test]
#[cfg(feature = "cranelift")]
fn bor_jit() { check_bor("--jit"); }

// --- bxor ---
const BXOR_SRC: &str = "f>n;bxor 12 10";

fn check_bxor(engine: &str) {
    assert_eq!(run(engine, BXOR_SRC, "f"), "6", "engine={engine}");
}

#[test]
fn bxor_vm() { check_bxor("--vm"); }
#[test]
#[cfg(feature = "cranelift")]
fn bxor_jit() { check_bxor("--jit"); }

// --- bnot ---
const BNOT_ZERO_SRC: &str = "f>n;bnot 0";
const BNOT_ONE_SRC: &str = "f>n;bnot 1";

fn check_bnot_zero(engine: &str) {
    assert_eq!(run(engine, BNOT_ZERO_SRC, "f"), "4294967295", "engine={engine}");
}
fn check_bnot_one(engine: &str) {
    assert_eq!(run(engine, BNOT_ONE_SRC, "f"), "4294967294", "engine={engine}");
}

#[test]
fn bnot_zero_vm() { check_bnot_zero("--vm"); }
#[test]
fn bnot_one_vm() { check_bnot_one("--vm"); }
#[test]
#[cfg(feature = "cranelift")]
fn bnot_zero_jit() { check_bnot_zero("--jit"); }
#[test]
#[cfg(feature = "cranelift")]
fn bnot_one_jit() { check_bnot_one("--jit"); }

// --- bshl ---
const BSHL_SRC: &str = "f>n;bshl 1 4";

fn check_bshl(engine: &str) {
    assert_eq!(run(engine, BSHL_SRC, "f"), "16", "engine={engine}");
}

#[test]
fn bshl_vm() { check_bshl("--vm"); }
#[test]
#[cfg(feature = "cranelift")]
fn bshl_jit() { check_bshl("--jit"); }

// --- bshr ---
const BSHR_SRC: &str = "f>n;bshr 256 3";

fn check_bshr(engine: &str) {
    assert_eq!(run(engine, BSHR_SRC, "f"), "32", "engine={engine}");
}

#[test]
fn bshr_vm() { check_bshr("--vm"); }
#[test]
#[cfg(feature = "cranelift")]
fn bshr_jit() { check_bshr("--jit"); }

// --- brot ---
const BROT_1_1_SRC: &str = "f>n;brot 1 1";
const BROT_1_31_SRC: &str = "f>n;brot 1 31";

fn check_brot_1_1(engine: &str) {
    assert_eq!(run(engine, BROT_1_1_SRC, "f"), "2", "engine={engine}");
}
fn check_brot_1_31(engine: &str) {
    assert_eq!(run(engine, BROT_1_31_SRC, "f"), "2147483648", "engine={engine}");
}

#[test]
fn brot_1_1_vm() { check_brot_1_1("--vm"); }
#[test]
fn brot_1_31_vm() { check_brot_1_31("--vm"); }
#[test]
#[cfg(feature = "cranelift")]
fn brot_1_1_jit() { check_brot_1_1("--jit"); }
#[test]
#[cfg(feature = "cranelift")]
fn brot_1_31_jit() { check_brot_1_31("--jit"); }

// --- mod 2^32 wrap: values > 2^32 are truncated ---
// 2^32 = 4294967296; as u32 it wraps to 0. band(0, 1) = 0.
const MOD_WRAP_SRC: &str = "f>n;band 4294967296 1";

fn check_mod_wrap(engine: &str) {
    assert_eq!(run(engine, MOD_WRAP_SRC, "f"), "0", "engine={engine}");
}

#[test]
fn mod_wrap_vm() { check_mod_wrap("--vm"); }
#[test]
#[cfg(feature = "cranelift")]
fn mod_wrap_jit() { check_mod_wrap("--jit"); }

// --- shift amount mod 32: bshl 1 32 == bshl 1 0 == 1 ---
const SHIFT_MOD_SRC: &str = "f>n;bshl 1 32";

fn check_shift_mod(engine: &str) {
    assert_eq!(run(engine, SHIFT_MOD_SRC, "f"), "1", "engine={engine}");
}

#[test]
fn shift_mod_vm() { check_shift_mod("--vm"); }
#[test]
#[cfg(feature = "cranelift")]
fn shift_mod_jit() { check_shift_mod("--jit"); }
