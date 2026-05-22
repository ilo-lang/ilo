//! Regression tests for ILO-49: closure-heavy programs must run on the
//! Cranelift JIT without triggering the AArch64 near-call relocation
//! assertion in cranelift-jit 0.116 and falling back to the bytecode VM.
//!
//! Root cause: `cranelift-jit 0.116` on AArch64 asserted when a `bl`
//! immediate overflowed and no veneer was emitted (upstream
//! `bytecodealliance/wasmtime#12239`). The Phase 2 closure-capture lift
//! (PR #384 + PR #265) exercises this code path with inline lambdas
//! holding captured values.
//!
//! These tests verify:
//! 1. Closure-heavy programs produce correct output under `--jit`.
//! 2. No `[ilo:jit-fallback]` breadcrumb appears on stderr (i.e. the
//!    JIT compile-and-call path succeeded; no panic fallback was taken).
//!
//! We run the programs from `examples/closure-heavy.ilo` because that
//! file is the canonical in-context learning example for this bug.
//!
//! Gated on the `cranelift` cargo feature.

#![cfg(feature = "cranelift")]

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn example_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("closure-heavy.ilo")
}

/// Run an entry point from the closure-heavy example file under `--jit`.
/// Asserts correct stdout and no `[ilo:jit-fallback]` in stderr.
fn run_jit_no_fallback(entry: &str, args: &[&str]) -> String {
    let path = example_path();
    let mut cmd = ilo();
    cmd.arg(&path).arg("--jit").arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    assert!(
        out.status.success(),
        "ilo --jit {entry} failed: stderr={stderr:?}"
    );
    assert!(
        !stderr.contains("[ilo:jit-fallback]"),
        "JIT panic fallback detected for {entry} — closure-heavy JIT regression re-introduced. \
         stderr={stderr:?}"
    );
    stdout
}

#[test]
fn closure_heavy_pipeline_no_fallback() {
    // filter → sort-by-capture → map-scale: three inline closures with captures.
    let result = run_jit_no_fallback(
        "pipeline",
        &["[10,20,30,40,50,60,70,80,90,100]", "20", "80", "2"],
    );
    assert_eq!(result, "[100, 80, 120, 60, 140, 40, 160]");
}

#[test]
fn closure_heavy_double_shift_no_fallback() {
    // Two chained map closures each capturing a distinct variable.
    let result = run_jit_no_fallback("double-shift", &["[1,2,3]", "10", "3"]);
    assert_eq!(result, "[33, 36, 39]");
}

#[test]
fn closure_heavy_weighted_sum_no_fallback() {
    // fold reducer closure capturing a weight parameter.
    let result = run_jit_no_fallback("weighted-sum", &["[1,2,3,4]", "5"]);
    assert_eq!(result, "50");
}

#[test]
fn closure_heavy_sort_by_dist_no_fallback() {
    // Sort key closure capturing a target value.
    let result = run_jit_no_fallback("sort-by-dist", &["[1,5,10,20]", "8"]);
    assert_eq!(result, "[10, 5, 1, 20]");
}
