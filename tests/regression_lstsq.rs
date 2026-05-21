// Cross-engine regression tests for the `lstsq` ordinary-least-squares
// builtin. Exercises the tree-walking interpreter, the register VM, and
// (when available) the cranelift JIT. `lstsq` is tree-bridge eligible
// so VM and JIT dispatch through the tree path — the cross-engine
// coverage here protects the bridge wiring.

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

fn approx(engine: &str, src: &str, args: &[&str], expected: f64, tol: f64) {
    let actual = run_num(engine, src, args);
    assert!(
        (actual - expected).abs() < tol,
        "engine={engine} src=`{src}` args={args:?}: got {actual}, expected {expected} (tol={tol})"
    );
}

fn engines() -> Vec<&'static str> {
    let mut v = vec!["--vm"];
    if cfg!(feature = "cranelift") {
        v.push("--jit");
    }
    v
}

fn check(src: &str, args: &[&str], expected: f64) {
    for e in engines() {
        approx(e, src, args, expected, 1e-9);
    }
}

fn check_loose(src: &str, args: &[&str], expected: f64, tol: f64) {
    for e in engines() {
        approx(e, src, args, expected, tol);
    }
}

#[test]
fn lstsq_perfect_line_intercept() {
    // y = 2x + 3 over five points. lstsq returns [intercept, slope].
    let src = "f a:L (L n) b:L n>n;x=lstsq a b;x.0";
    let xm = "[[1,1],[1,2],[1,3],[1,4],[1,5]]";
    let ys = "[5,7,9,11,13]";
    check(src, &[xm, ys], 3.0);
}

#[test]
fn lstsq_perfect_line_slope() {
    let src = "f a:L (L n) b:L n>n;x=lstsq a b;x.1";
    let xm = "[[1,1],[1,2],[1,3],[1,4],[1,5]]";
    let ys = "[5,7,9,11,13]";
    check(src, &[xm, ys], 2.0);
}

#[test]
fn lstsq_intercept_only_is_mean() {
    // Best constant fit is the mean of ys.
    let src = "f a:L (L n) b:L n>n;x=lstsq a b;x.0";
    let xm = "[[1],[1],[1],[1],[1]]";
    let ys = "[2,4,6,8,10]";
    check(src, &[xm, ys], 6.0);
}

#[test]
fn lstsq_multivariate_exact_fit() {
    // z = 1 + 2x + 3y over four sample points. Exact fit, no noise.
    let src_i = "f a:L (L n) b:L n>n;x=lstsq a b;x.0";
    let src_x = "f a:L (L n) b:L n>n;x=lstsq a b;x.1";
    let src_y = "f a:L (L n) b:L n>n;x=lstsq a b;x.2";
    let xm = "[[1,1,1],[1,2,1],[1,1,2],[1,2,2]]";
    let ys = "[6,8,9,11]";
    check(src_i, &[xm, ys], 1.0);
    check(src_x, &[xm, ys], 2.0);
    check(src_y, &[xm, ys], 3.0);
}

#[test]
fn lstsq_overdetermined_noisy_fit() {
    // Build noisy y = 2x + 3 over 100 points using rndn with a fixed
    // seed. The recovered slope/intercept should land within a coarse
    // tolerance of the ground truth despite the noise.
    //
    // Layered as one program so the seeded rng state is preserved
    // between the design-matrix construction and the noise injection.
    let src_slope = r#"f>n;
        n=100;
        xs=range 0 n;
        ys=map (x:n>n;+ (+ 3 (* 2 x)) (* 0.5 (rndn 0 1))) xs;
        xm=map (x:n>L n;[1, x]) xs;
        b=lstsq xm ys;
        b.1"#;
    let src_int = r#"f>n;
        n=100;
        xs=range 0 n;
        ys=map (x:n>n;+ (+ 3 (* 2 x)) (* 0.5 (rndn 0 1))) xs;
        xm=map (x:n>L n;[1, x]) xs;
        b=lstsq xm ys;
        b.0"#;
    // Slope is well-conditioned given x spans 0..100; tolerance is
    // intentionally loose because rndn distributions differ per engine.
    check_loose(src_slope, &[], 2.0, 0.1);
    check_loose(src_int, &[], 3.0, 5.0);
}

#[test]
fn lstsq_single_row_single_col() {
    // 1x1 system: one sample, one feature. Closed-form result is ys[0] / xm[0][0].
    let src = "f a:L (L n) b:L n>n;x=lstsq a b;x.0";
    check(src, &["[[2]]", "[10]"], 5.0);
}

#[test]
fn lstsq_underdetermined_errors() {
    // More columns than rows: normal-equation OLS is not defined.
    let src = "f a:L (L n) b:L n>n;x=lstsq a b;x.0";
    for e in engines() {
        let (ok, stdout, _stderr) = run_text(e, src, &["[[1,2,3]]", "[1]"]);
        let failed = !ok || stdout == "nil";
        assert!(
            failed,
            "engine={e}: expected lstsq on underdetermined system to fail or return nil, got `{stdout}`"
        );
    }
}

#[test]
fn lstsq_dimension_mismatch_errors() {
    // ys length must equal row count of xm.
    let src = "f a:L (L n) b:L n>n;x=lstsq a b;x.0";
    for e in engines() {
        let (ok, stdout, _stderr) = run_text(e, src, &["[[1,1],[1,2],[1,3]]", "[1,2]"]);
        let failed = !ok || stdout == "nil";
        assert!(
            failed,
            "engine={e}: expected lstsq with mismatched dims to fail or return nil, got `{stdout}`"
        );
    }
}

#[test]
fn lstsq_rank_deficient_errors() {
    // Two identical columns -> XᵀX is singular. solve raises ILO-R009;
    // the cranelift JIT returns nil through the bridge unwrap path.
    let src = "f a:L (L n) b:L n>n;x=lstsq a b;x.0";
    for e in engines() {
        let (ok, stdout, _stderr) = run_text(e, src, &["[[1,1],[1,1],[1,1],[1,1]]", "[1,2,3,4]"]);
        let failed = !ok || stdout == "nil";
        assert!(
            failed,
            "engine={e}: expected lstsq on rank-deficient design to fail or return nil, got `{stdout}`"
        );
    }
}

#[test]
fn lstsq_endpoints_equal() {
    // Edge: ys all the same. Best line through ys=[5,5,5,5,5] is y=5
    // (intercept 5, slope 0). Tests the constant-target boundary case.
    let src_i = "f a:L (L n) b:L n>n;x=lstsq a b;x.0";
    let src_s = "f a:L (L n) b:L n>n;x=lstsq a b;x.1";
    let xm = "[[1,1],[1,2],[1,3],[1,4],[1,5]]";
    let ys = "[5,5,5,5,5]";
    check(src_i, &[xm, ys], 5.0);
    check(src_s, &[xm, ys], 0.0);
}
