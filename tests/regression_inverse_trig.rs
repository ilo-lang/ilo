// Cross-engine smoke tests for the inverse trig builtins (asin, acos, atan).
// Each is checked against tree, vm, and cranelift (when enabled) to f64
// precision. Mirrors regression_math_extra.rs so the geospatial personas
// no longer need a hand-rolled 6-term Taylor series for asin.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_num(engine: &str, src: &str) -> f64 {
    let out = ilo()
        .args([src, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<f64>()
        .expect("expected numeric output")
}

fn approx(engine: &str, src: &str, expected: f64) {
    let actual = run_num(engine, src);
    assert!(
        (actual - expected).abs() < 1e-12,
        "engine={engine} src=`{src}`: got {actual}, expected {expected}"
    );
}

fn check_all(src: &str, expected: f64) {
    approx("--run-tree", src, expected);
    approx("--run-vm", src, expected);
    #[cfg(feature = "cranelift")]
    approx("--run-cranelift", src, expected);
}

#[test]
fn asin_zero() {
    check_all("f>n;asin 0", 0.0);
}

#[test]
fn asin_one() {
    check_all("f>n;asin 1", std::f64::consts::FRAC_PI_2);
}

#[test]
fn asin_half() {
    // asin(0.5) = pi/6
    check_all("f>n;asin 0.5", std::f64::consts::FRAC_PI_6);
}

#[test]
fn asin_neg_half() {
    check_all("f>n;asin -0.5", -std::f64::consts::FRAC_PI_6);
}

#[test]
fn acos_zero() {
    check_all("f>n;acos 0", std::f64::consts::FRAC_PI_2);
}

#[test]
fn acos_one() {
    check_all("f>n;acos 1", 0.0);
}

#[test]
fn acos_half() {
    // acos(0.5) = pi/3
    check_all("f>n;acos 0.5", std::f64::consts::FRAC_PI_3);
}

#[test]
fn acos_neg_one() {
    check_all("f>n;acos -1", std::f64::consts::PI);
}

#[test]
fn atan_zero() {
    check_all("f>n;atan 0", 0.0);
}

#[test]
fn atan_one() {
    check_all("f>n;atan 1", std::f64::consts::FRAC_PI_4);
}

#[test]
fn atan_neg_one() {
    check_all("f>n;atan -1", -std::f64::consts::FRAC_PI_4);
}

#[test]
fn atan_sqrt3() {
    // atan(sqrt(3)) = pi/3 (60 degrees).
    check_all("f>n;atan 1.7320508075688772", std::f64::consts::FRAC_PI_3);
}

#[test]
fn asin_round_trip() {
    // sin(asin(x)) = x for x in [-1, 1].
    check_all("f>n;a=asin 0.3;sin a", 0.3);
    check_all("f>n;a=asin 0.9;sin a", 0.9);
}

#[test]
fn acos_round_trip() {
    // cos(acos(x)) = x for x in [-1, 1].
    check_all("f>n;a=acos 0.25;cos a", 0.25);
}

#[test]
fn atan_round_trip() {
    // tan(atan(x)) = x for all real x.
    check_all("f>n;a=atan 0.7;tan a", 0.7);
    check_all("f>n;a=atan 5.5;tan a", 5.5);
}

#[test]
fn asin_domain_error_is_nan() {
    // asin is defined on [-1, 1]; outside the domain the result is NaN.
    // Across all engines the same NaN should appear.
    for engine in ["--run-tree", "--run-vm"] {
        let actual = run_num(engine, "f>n;asin 2");
        assert!(
            actual.is_nan(),
            "engine={engine}: expected NaN for asin(2), got {actual}"
        );
    }
    #[cfg(feature = "cranelift")]
    {
        let actual = run_num("--run-cranelift", "f>n;asin 2");
        assert!(actual.is_nan(), "cranelift: expected NaN for asin(2)");
    }
}
