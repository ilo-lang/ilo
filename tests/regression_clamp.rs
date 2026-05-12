// Cross-engine smoke tests for the `clamp` builtin.
// `clamp x lo hi` returns max(lo, min(hi, x)). When lo > hi, the
// outer max wins so the result is always >= lo (documented choice).

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

fn check_all(src: &str, expected: f64) {
    for engine in ["--run-tree", "--run-vm"] {
        let actual = run_num(engine, src);
        assert!(
            (actual - expected).abs() < 1e-12,
            "engine={engine} src=`{src}`: got {actual}, expected {expected}"
        );
    }
    #[cfg(feature = "cranelift")]
    {
        let engine = "--run-cranelift";
        let actual = run_num(engine, src);
        assert!(
            (actual - expected).abs() < 1e-12,
            "engine={engine} src=`{src}`: got {actual}, expected {expected}"
        );
    }
}

#[test]
fn clamp_inside_range() {
    check_all("f>n;clamp 5 0 10", 5.0);
}

#[test]
fn clamp_below_low_bound() {
    check_all("f>n;clamp -3 0 10", 0.0);
}

#[test]
fn clamp_above_high_bound() {
    check_all("f>n;clamp 15 0 10", 10.0);
}

#[test]
fn clamp_at_low_boundary() {
    check_all("f>n;clamp 0 0 10", 0.0);
}

#[test]
fn clamp_at_high_boundary() {
    check_all("f>n;clamp 10 0 10", 10.0);
}

#[test]
fn clamp_float_inside() {
    check_all("f>n;clamp 0.5 0 1", 0.5);
}

#[test]
fn clamp_inverted_bounds_returns_low() {
    // Documented semantic: when lo > hi, result == lo (the outer max wins).
    check_all("f>n;clamp 5 10 0", 10.0);
}

#[test]
fn clamp_inverted_bounds_negative_x_returns_low() {
    // Even with x below both bounds, the outer max with lo wins when
    // lo > hi. Cross-checks the inverted-bounds semantic on a separate x.
    check_all("f>n;clamp -3 10 0", 10.0);
}
