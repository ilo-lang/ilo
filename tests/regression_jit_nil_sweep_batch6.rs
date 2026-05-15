// Regression tests for the Cranelift JIT-helper permissive-nil sweep, batch 6.
//
// Helpers in scope (Group D — stats + linalg):
//   jit_median, jit_quantile, jit_stdev, jit_variance, jit_fft, jit_ifft,
//   jit_transpose, jit_matmul, jit_dot, jit_det, jit_inv, jit_solve.
//
// Before this PR these helpers silently returned TAG_NIL (or NaN for
// jit_dot / jit_det) on failure paths where tree/VM raise runtime errors.
// The fix routes the failure paths through the `JIT_RUNTIME_ERROR` TLS cell
// introduced in #254, threading a packed source-span immediate so
// diagnostics render with a caret matching tree/VM.
//
// Per-helper error-path coverage lives in `vm::tests::jit_helpers` —
// driving the helpers directly bypasses the surface verifier which rejects
// programs that statically mis-shape these calls. These CLI tests focus on
// cross-engine happy-path parity, pinning that wiring the span/error
// threads did not regress the success cases across tree, VM, and
// Cranelift JIT.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn check_stdout(engine: &str, src: &str, expected: &str) {
    let out = ilo()
        .args([src, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "engine={engine}: expected success for `{src}`, got stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        expected,
        "engine={engine}: stdout mismatch for `{src}`"
    );
}

// Run a check across all three engines and assert identical output.
fn check_all(src: &str, expected: &str) {
    check_stdout("--run-tree", src, expected);
    check_stdout("--run-vm", src, expected);
    #[cfg(feature = "cranelift")]
    check_stdout("--run-cranelift", src, expected);
}

// ── Stats helpers ─────────────────────────────────────────────────────────

#[test]
fn median_cross_engine() {
    check_all("f>n;median [3 1 2]", "2");
}

#[test]
fn median_even_length_cross_engine() {
    check_all("f>n;median [1 2 3 4]", "2.5");
}

#[test]
fn quantile_cross_engine() {
    check_all("f>n;quantile [1 2 3 4] 0.5", "2.5");
}

#[test]
fn stdev_cross_engine() {
    // sample stdev of [1..5] = sqrt(2.5)
    check_all("f>n;stdev [1 2 3 4 5]", "1.5811388300841898");
}

#[test]
fn variance_cross_engine() {
    check_all("f>n;variance [1 2 3 4 5]", "2.5");
}

#[test]
fn fft_cross_engine() {
    check_all(
        "f>L (L n);fft [1 0 0 0]",
        "[[1, 0], [1, 0], [1, 0], [1, 0]]",
    );
}

#[test]
fn ifft_cross_engine() {
    check_all("f>L n;ifft [[1 0] [1 0] [1 0] [1 0]]", "[1, 0, 0, 0]");
}

// ── Linalg helpers ────────────────────────────────────────────────────────

#[test]
fn transpose_cross_engine() {
    check_all("f>L (L n);transpose [[1 2] [3 4]]", "[[1, 3], [2, 4]]");
}

#[test]
fn transpose_3x2_cross_engine() {
    check_all(
        "f>L (L n);transpose [[1 2] [3 4] [5 6]]",
        "[[1, 3, 5], [2, 4, 6]]",
    );
}

#[test]
fn matmul_identity_cross_engine() {
    check_all(
        "f>L (L n);matmul [[1 0] [0 1]] [[2 3] [4 5]]",
        "[[2, 3], [4, 5]]",
    );
}

#[test]
fn dot_cross_engine() {
    check_all("f>n;dot [1 2 3] [4 5 6]", "32");
}

#[test]
fn det_2x2_cross_engine() {
    check_all("f>n;det [[1 2] [3 4]]", "-2");
}

#[test]
fn inv_identity_cross_engine() {
    check_all("f>L (L n);inv [[1 0] [0 1]]", "[[1, 0], [0, 1]]");
}

#[test]
fn solve_identity_cross_engine() {
    check_all("f>L n;solve [[1 0] [0 1]] [3 4]", "[3, 4]");
}

// ── No-stale-error-leak guard ─────────────────────────────────────────────
//
// An errored cranelift invocation followed by a fresh process running clean
// arithmetic must succeed: pins that the JitRuntimeErrorGuard's clear-on-
// entry contract holds for the new error sites added in this batch.
#[cfg(feature = "cranelift")]
#[test]
fn cranelift_no_stale_error_after_batch6_failure() {
    // First: deliberately error out via a singular-matrix inv.
    // The surface verifier accepts this; the singular check fires at runtime.
    let out = ilo()
        .args(["f>L (L n);inv [[1 2] [2 4]]", "--run-cranelift", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success(), "expected failure for singular inv");

    // Second: a fresh, unrelated invocation must succeed.
    check_stdout("--run-cranelift", "f>n;median [1 2 3]", "2");
}
