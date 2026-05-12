// Cross-engine smoke tests for the advanced linear-algebra builtins
// (solve, inv, det). These exercise the tree-walking interpreter, the
// register VM, and (when available) the cranelift JIT.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_text(engine: &str, src: &str, args: &[&str]) -> (bool, String, String) {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine).arg("f");
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn run_num(engine: &str, src: &str, args: &[&str]) -> f64 {
    let (ok, stdout, stderr) = run_text(engine, src, args);
    assert!(
        ok,
        "ilo {engine} failed for `{src}` args={args:?}: stderr={stderr}"
    );
    stdout
        .parse::<f64>()
        .unwrap_or_else(|_| panic!("expected numeric output, got `{stdout}` (stderr={stderr})"))
}

fn approx_num(engine: &str, src: &str, args: &[&str], expected: f64) {
    let actual = run_num(engine, src, args);
    assert!(
        (actual - expected).abs() < 1e-10,
        "engine={engine} src=`{src}` args={args:?}: got {actual}, expected {expected}"
    );
}

fn approx_num_loose(engine: &str, src: &str, args: &[&str], expected: f64, tol: f64) {
    let actual = run_num(engine, src, args);
    assert!(
        (actual - expected).abs() < tol,
        "engine={engine} src=`{src}` args={args:?}: got {actual}, expected {expected} (tol={tol})"
    );
}

fn engines() -> Vec<&'static str> {
    let mut v = vec!["--run-tree", "--run-vm"];
    if cfg!(feature = "cranelift") {
        v.push("--run-cranelift");
    }
    v
}

fn check_num(src: &str, args: &[&str], expected: f64) {
    for e in engines() {
        approx_num(e, src, args, expected);
    }
}

#[test]
fn det_identity_2x2() {
    check_num("f a:L (L n)>n;det a", &["[[1,0],[0,1]]"], 1.0);
}

#[test]
fn det_2x2_simple() {
    check_num("f a:L (L n)>n;det a", &["[[2,1],[1,3]]"], 5.0);
}

#[test]
fn det_3x3() {
    // det of upper-triangular = product of diagonal = 1*2*3 = 6
    check_num("f a:L (L n)>n;det a", &["[[1,2,3],[0,2,5],[0,0,3]]"], 6.0);
}

#[test]
fn det_singular_near_zero() {
    // Singular: row 2 = 2*row 1
    for e in engines() {
        approx_num_loose(e, "f a:L (L n)>n;det a", &["[[1,2],[2,4]]"], 0.0, 1e-9);
    }
}

#[test]
fn solve_2x2_identity_swap() {
    // [[1,1],[1,-1]] x = [3,1] → x = [2,1]
    let src = "f a:L (L n) b:L n>n;x=solve a b;x.0";
    let src2 = "f a:L (L n) b:L n>n;x=solve a b;x.1";
    for e in engines() {
        approx_num(e, src, &["[[1,1],[1,-1]]", "[3,1]"], 2.0);
        approx_num(e, src2, &["[[1,1],[1,-1]]", "[3,1]"], 1.0);
    }
}

#[test]
fn inv_diag_2x2() {
    // inv [[2,0],[0,2]] = [[0.5,0],[0,0.5]] — pick element (0,0) and (1,1)
    let src00 = "f a:L (L n)>n;m=inv a;r=m.0;r.0";
    let src11 = "f a:L (L n)>n;m=inv a;r=m.1;r.1";
    let src01 = "f a:L (L n)>n;m=inv a;r=m.0;r.1";
    for e in engines() {
        approx_num(e, src00, &["[[2,0],[0,2]]"], 0.5);
        approx_num(e, src11, &["[[2,0],[0,2]]"], 0.5);
        approx_num(e, src01, &["[[2,0],[0,2]]"], 0.0);
    }
}

#[test]
fn inv_singular_errors() {
    // The tree-walking interpreter and VM return a runtime error; the
    // cranelift JIT (which lacks a runtime-error path for matrix helpers)
    // returns `nil` instead. Accept either as "did not produce a real
    // inverse for a singular matrix".
    let src = "f a:L (L n)>n;m=inv a;r=m.0;r.0";
    for e in engines() {
        let (ok, stdout, _stderr) = run_text(e, src, &["[[1,2],[2,4]]"]);
        let failed = !ok || stdout == "nil";
        assert!(
            failed,
            "engine={e}: expected inv on singular matrix to fail or return nil, got `{stdout}`"
        );
    }
}

#[test]
fn det_non_square_errors() {
    let src = "f a:L (L n)>n;det a";
    for e in engines() {
        let (ok, stdout, _stderr) = run_text(e, src, &["[[1,2,3],[4,5,6]]"]);
        // tree/vm raise; cranelift currently returns NaN as a sentinel.
        let failed = !ok || stdout.contains("NaN") || stdout.contains("nan");
        assert!(
            failed,
            "engine={e}: expected det on non-square matrix to fail or return nan, got `{stdout}`"
        );
    }
}
