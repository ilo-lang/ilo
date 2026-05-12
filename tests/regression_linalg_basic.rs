// Cross-engine smoke tests for the basic linalg builtins
// (transpose, matmul, dot). Each is checked against tree, vm, and
// cranelift, mirroring regression_math_extra.rs.
//
// Manifesto rationale: hand-rolled linalg risks silent precision loss
// (same class as `expx` Taylor). These builtins delegate to vetted
// host-language arithmetic so users don't reimplement them per-program.

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
    assert_eq!(run_ok("--run-tree", src), expected, "tree engine");
    assert_eq!(run_ok("--run-vm", src), expected, "vm engine");
    #[cfg(feature = "cranelift")]
    assert_eq!(run_ok("--run-cranelift", src), expected, "cranelift engine");
}

fn check_all_num(src: &str, expected: f64) {
    for engine in &["--run-tree", "--run-vm"] {
        let actual = run_ok(engine, src).parse::<f64>().expect("number");
        assert!(
            (actual - expected).abs() < 1e-10,
            "engine={engine} src=`{src}`: got {actual}, expected {expected}"
        );
    }
    #[cfg(feature = "cranelift")]
    {
        let actual = run_ok("--run-cranelift", src)
            .parse::<f64>()
            .expect("number");
        assert!(
            (actual - expected).abs() < 1e-10,
            "engine=cranelift src=`{src}`: got {actual}, expected {expected}"
        );
    }
}

// ── transpose ───────────────────────────────────────────────────────

#[test]
fn transpose_2x2() {
    check_all_str("f>L (L n);transpose [[1,2],[3,4]]", "[[1, 3], [2, 4]]");
}

#[test]
fn transpose_2x3_yields_3x2() {
    check_all_str(
        "f>L (L n);transpose [[1,2,3],[4,5,6]]",
        "[[1, 4], [2, 5], [3, 6]]",
    );
}

#[test]
fn transpose_ragged_errors() {
    // tree + vm catch the ragged shape at runtime.
    let err_tree = run_err("--run-tree", "f>L (L n);transpose [[1,2],[3]]");
    assert!(
        err_tree.contains("transpose") || err_tree.contains("ragged"),
        "tree: got: {err_tree}"
    );
    let err_vm = run_err("--run-vm", "f>L (L n);transpose [[1,2],[3]]");
    assert!(
        err_vm.contains("transpose") || err_vm.contains("ragged"),
        "vm: got: {err_vm}"
    );
}

// ── matmul ──────────────────────────────────────────────────────────

#[test]
fn matmul_2x3_by_3x2_gives_2x2() {
    check_all_str(
        "f>L (L n);matmul [[1,2,3],[4,5,6]] [[7,8],[9,10],[11,12]]",
        "[[58, 64], [139, 154]]",
    );
}

#[test]
fn matmul_identity_2x2() {
    check_all_str(
        "f>L (L n);matmul [[1,0],[0,1]] [[5,6],[7,8]]",
        "[[5, 6], [7, 8]]",
    );
}

#[test]
fn matmul_shape_mismatch_errors() {
    // 2x3 * 2x2 is invalid (cols(a)=3 != rows(b)=2).
    let err_tree = run_err(
        "--run-tree",
        "f>L (L n);matmul [[1,2,3],[4,5,6]] [[1,2],[3,4]]",
    );
    assert!(
        err_tree.contains("matmul") || err_tree.contains("shape"),
        "tree: got: {err_tree}"
    );
    let err_vm = run_err(
        "--run-vm",
        "f>L (L n);matmul [[1,2,3],[4,5,6]] [[1,2],[3,4]]",
    );
    assert!(
        err_vm.contains("matmul") || err_vm.contains("shape"),
        "vm: got: {err_vm}"
    );
}

// ── dot ─────────────────────────────────────────────────────────────

#[test]
fn dot_basic() {
    // 1*4 + 2*5 + 3*6 = 4 + 10 + 18 = 32
    check_all_num("f>n;dot [1,2,3] [4,5,6]", 32.0);
}

#[test]
fn dot_with_negatives() {
    // 1*-1 + 2*1 = 1
    check_all_num("f>n;dot [1,2] [-1,1]", 1.0);
}

#[test]
fn dot_length_mismatch_errors() {
    let err_tree = run_err("--run-tree", "f>n;dot [1,2,3] [1,2]");
    assert!(
        err_tree.contains("dot") || err_tree.contains("length"),
        "tree: got: {err_tree}"
    );
    let err_vm = run_err("--run-vm", "f>n;dot [1,2,3] [1,2]");
    assert!(
        err_vm.contains("dot") || err_vm.contains("length"),
        "vm: got: {err_vm}"
    );
}
