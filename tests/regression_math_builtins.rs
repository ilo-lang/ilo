// Cross-engine smoke tests for the transcendental math builtins
// (pow, sqrt, log, exp, sin, cos). Each is checked against tree, vm,
// and cranelift, replacing the hand-rolled Taylor-series approximations
// that previously silently miscompiled for large x.

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
        (actual - expected).abs() < 1e-10,
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
fn pow_integer_exponent() {
    check_all("f>n;pow 2 10", 1024.0);
}

#[test]
fn pow_fractional_exponent() {
    check_all("f>n;pow 4 0.5", 2.0);
}

#[test]
fn sqrt_basic() {
    check_all("f>n;sqrt 2", std::f64::consts::SQRT_2);
}

#[test]
fn sqrt_large() {
    // The motivating regression: large arguments must stay accurate.
    check_all("f>n;sqrt 1000000000000", 1_000_000.0);
}

#[test]
fn exp_basic() {
    check_all("f>n;exp 1", std::f64::consts::E);
}

#[test]
fn log_basic() {
    check_all("f>n;log 2.718281828459045", 1.0);
}

#[test]
fn log_exp_round_trip() {
    // log/exp via intermediate binding (parser doesn't nest unary calls).
    check_all("f>n;a=exp 5;log a", 5.0);
}

#[test]
fn sin_zero() {
    check_all("f>n;sin 0", 0.0);
}

#[test]
fn cos_zero() {
    check_all("f>n;cos 0", 1.0);
}

#[test]
fn sin_large_argument() {
    // The exact pain point: sin(100000) used to wander off via hand-rolled Taylor.
    // std::f64::sin handles the range reduction correctly.
    check_all("f>n;sin 100000", (100_000.0_f64).sin());
}
