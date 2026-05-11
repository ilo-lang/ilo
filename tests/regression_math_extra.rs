// Cross-engine smoke tests for the extra transcendental math builtins
// (tan, log10, log2, atan2). Each is checked against tree, vm, and
// cranelift, mirroring regression_math_builtins.rs.

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
fn tan_zero() {
    check_all("f>n;tan 0", 0.0);
}

#[test]
fn tan_quarter_pi() {
    check_all("f>n;tan 0.7853981633974483", 1.0);
}

#[test]
fn log10_hundred() {
    check_all("f>n;log10 100", 2.0);
}

#[test]
fn log10_one() {
    check_all("f>n;log10 1", 0.0);
}

#[test]
fn log2_eight() {
    check_all("f>n;log2 8", 3.0);
}

#[test]
fn log2_one() {
    check_all("f>n;log2 1", 0.0);
}

#[test]
fn atan2_first_quadrant() {
    // atan2(y=1, x=1) = pi/4 (first quadrant)
    check_all("f>n;atan2 1 1", std::f64::consts::FRAC_PI_4);
}

#[test]
fn atan2_second_quadrant() {
    // atan2(y=1, x=-1) = 3*pi/4 (second quadrant, positive y, negative x)
    check_all("f>n;atan2 1 -1", 3.0 * std::f64::consts::FRAC_PI_4);
}

#[test]
fn atan2_third_quadrant() {
    // atan2(y=-1, x=-1) = -3*pi/4 (third quadrant, both negative)
    check_all("f>n;atan2 -1 -1", -3.0 * std::f64::consts::FRAC_PI_4);
}

#[test]
fn atan2_fourth_quadrant() {
    // atan2(y=-1, x=1) = -pi/4 (fourth quadrant, negative y, positive x)
    check_all("f>n;atan2 -1 1", -std::f64::consts::FRAC_PI_4);
}

#[test]
fn atan2_argument_order() {
    // Confirms y-first, x-second order (C/Python convention).
    // atan2(0, 1) = 0 (point on positive x-axis)
    // atan2(1, 0) = pi/2 (point on positive y-axis)
    check_all("f>n;atan2 0 1", 0.0);
    check_all("f>n;atan2 1 0", std::f64::consts::FRAC_PI_2);
}
