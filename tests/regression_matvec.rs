#![allow(clippy::single_element_loop)] // see soft-deprecate-tree: arrays shrank from 2-3 engines to 1
// Cross-engine smoke tests for the `matvec` builtin.
//
// matvec xm ys > L n  — native matrix-vector multiply. Replaces the
// `flatten matmul xm (map (y:n>L n;[y]) ys)` wrap-as-column ceremony
// every linear-regression-style persona was paying.
//
// Mirrors regression_linalg_basic.rs in shape: tree/vm/cranelift smoke
// for happy paths, and an error-message contains-check for the ILO-R009
// failure modes (dim mismatch, empty, ragged).

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_ok(engine: &str, src: &str) -> String {
    let out = ilo()
        .args([src, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(engine: &str, src: &str) -> String {
    let out = ilo()
        .args([src, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} unexpectedly succeeded for `{src}`: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn check_all_str(src: &str, expected: &str) {
    assert_eq!(run_ok("--vm", src), expected, "vm engine");
    #[cfg(feature = "cranelift")]
    assert_eq!(run_ok("--jit", src), expected, "cranelift engine");
}

fn check_all_err(src: &str, needle: &str) {
    // VM surfaces the runtime error; JIT/AOT currently coerces runtime
    // errors into `nil` for tree-bridged builtins (same pattern as the
    // matmul / transpose error tests in regression_linalg_basic.rs).
    // The bridge contract is "value or err-propagation"; only VM
    // observes the err today.
    let err_vm = run_err("--vm", src);
    assert!(
        err_vm.contains(needle),
        "vm: expected {needle:?} in: {err_vm}"
    );
}

// ── happy paths ─────────────────────────────────────────────────────

#[test]
fn matvec_identity_3x3() {
    // I * [4,5,6] = [4,5,6]
    check_all_str(
        "f>L n;matvec [[1,0,0],[0,1,0],[0,0,1]] [4,5,6]",
        "[4, 5, 6]",
    );
}

#[test]
fn matvec_2x2_basic() {
    // [[1,2],[3,4]] * [5,6] = [1*5+2*6, 3*5+4*6] = [17, 39]
    check_all_str("f>L n;matvec [[1,2],[3,4]] [5,6]", "[17, 39]");
}

#[test]
fn matvec_2x3() {
    // [[1,2,3],[4,5,6]] * [7,8,9] = [50, 122]
    check_all_str("f>L n;matvec [[1,2,3],[4,5,6]] [7,8,9]", "[50, 122]");
}

#[test]
fn matvec_1x1() {
    check_all_str("f>L n;matvec [[3]] [4]", "[12]");
}

#[test]
fn matvec_negatives_and_zeros() {
    // [[1,-1],[0,2]] * [3,4] = [3-4, 0+8] = [-1, 8]
    check_all_str("f>L n;matvec [[1,-1],[0,2]] [3,4]", "[-1, 8]");
}

#[test]
fn matvec_matches_flatten_matmul_ceremony() {
    // The recipe matvec replaces: flatten matmul xm (map (y:n>L n;[y]) ys).
    // Both should produce the same flat vector for the same inputs.
    let direct = run_ok("--vm", "f>L n;matvec [[1,2,3],[4,5,6]] [7,8,9]");
    let ceremony = run_ok(
        "--vm",
        "f>L n;flat (matmul [[1,2,3],[4,5,6]] (map (y:n>L n;[y]) [7,8,9]))",
    );
    assert_eq!(direct, ceremony, "matvec must match flat+matmul ceremony");
}

// ── error paths (ILO-R009) ──────────────────────────────────────────

#[test]
fn matvec_dim_mismatch_errors() {
    // matrix has 3 cols, ys has 2.
    check_all_err("f>L n;matvec [[1,2,3],[4,5,6]] [1,2]", "matvec");
}

#[test]
fn matvec_empty_matrix_errors() {
    // Empty top-level list. Surfaces as ILO-R009 from the matvec arm.
    // (The verifier may also flag the shape; the runtime error path is
    // what guards a programmatic empty-result.)
    let err = run_err("--vm", "f>L n;matvec [] []");
    assert!(
        err.contains("matvec") || err.contains("ILO-"),
        "expected matvec error, got: {err}"
    );
}

#[test]
fn matvec_ragged_rows_errors() {
    // Second row is shorter than the first.
    check_all_err("f>L n;matvec [[1,2,3],[4,5]] [1,2,3]", "matvec");
}
