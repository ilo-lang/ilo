// Cross-engine statistical tests for the rndn (normal-distribution) builtin.
// Each engine has its own RNG state, so we can't assert exact equality;
// instead we check that empirical mean/stdev over 1000 samples fall within
// tolerances that flakiness is effectively impossible.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_text(engine: &str, src: &str) -> String {
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

fn parse_pair(s: &str) -> (f64, f64) {
    // Output format: "mean stdev" (text concatenation with space)
    let parts: Vec<&str> = s.split_whitespace().collect();
    assert_eq!(parts.len(), 2, "expected `mean stdev`, got: {s}");
    let m: f64 = parts[0].parse().expect("mean not a number");
    let d: f64 = parts[1].parse().expect("stdev not a number");
    (m, d)
}

// ilo source: sample N times from N(mu, sigma), return "<mean> <stdev>".
// fld over 0..N would require an extra helper; use a while loop with two
// running sums (sum and sum of squares) so we get both mean and stdev.
fn mc_src(n: i64, mu: f64, sigma: f64) -> String {
    format!(
        "f>t;s=0;q=0;i=0;wh <i {n}{{x=rndn {mu} {sigma};s=+s x;sq=*x x;q=+q sq;i=+i 1}};m=/s {n};v=/q {n};mm=*m m;vr=-v mm;sd=sqrt vr;a=str m;b=str sd;cat [a,b] \" \""
    )
}

fn check_normal_stats(
    engine: &str,
    n: i64,
    mu: f64,
    sigma: f64,
    mean_tol: f64,
    stdev_range: (f64, f64),
) {
    let src = mc_src(n, mu, sigma);
    let out = run_text(engine, &src);
    let (m, d) = parse_pair(&out);
    assert!(
        (m - mu).abs() < mean_tol,
        "engine={engine}: mean {m} not within {mean_tol} of {mu} (out=`{out}`)"
    );
    assert!(
        d >= stdev_range.0 && d <= stdev_range.1,
        "engine={engine}: stdev {d} not in {stdev_range:?} (out=`{out}`)"
    );
}

// Standard error of the mean is sigma / sqrt(n). For n=1000 sigma=1
// that's 0.0316; 5 SE = 0.158, so tolerance of 0.2 is comfortably wide.
// For n=1000 sigma=2, 5 SE = 0.316, tolerance 0.4.
//
// Sample stdev: standard error of sample stdev ~ sigma / sqrt(2n) = 0.022;
// 5 SE = 0.11, so [0.85, 1.15] is comfortable for sigma=1.

const N: i64 = 1000;

#[test]
fn rndn_std_normal_tree() {
    check_normal_stats("--run-tree", N, 0.0, 1.0, 0.2, (0.85, 1.15));
}

#[test]
fn rndn_std_normal_vm() {
    check_normal_stats("--run-vm", N, 0.0, 1.0, 0.2, (0.85, 1.15));
}

#[cfg(feature = "cranelift")]
#[test]
fn rndn_std_normal_cranelift() {
    check_normal_stats("--run-cranelift", N, 0.0, 1.0, 0.2, (0.85, 1.15));
}

#[test]
fn rndn_shifted_scaled_tree() {
    // N(10, 2): mean tol 0.4 (5 SE), stdev in [1.7, 2.3].
    check_normal_stats("--run-tree", N, 10.0, 2.0, 0.4, (1.7, 2.3));
}

#[test]
fn rndn_shifted_scaled_vm() {
    check_normal_stats("--run-vm", N, 10.0, 2.0, 0.4, (1.7, 2.3));
}

#[cfg(feature = "cranelift")]
#[test]
fn rndn_shifted_scaled_cranelift() {
    check_normal_stats("--run-cranelift", N, 10.0, 2.0, 0.4, (1.7, 2.3));
}

#[test]
fn rndn_returns_number_type() {
    // Single-sample sanity: should be a finite number.
    let out = run_text("--run-tree", "f>n;rndn 0 1");
    let v: f64 = out.parse().expect("not a number");
    assert!(v.is_finite(), "rndn produced non-finite: {v}");
}

#[test]
fn rndn_zero_sigma_returns_mu() {
    // Box-Muller with sigma=0: mu + 0*z = mu exactly.
    let out = run_text("--run-tree", "f>n;rndn 7 0");
    let v: f64 = out.parse().expect("not a number");
    assert_eq!(v, 7.0, "rndn 7 0 should be exactly 7, got {v}");
}
