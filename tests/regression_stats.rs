// Cross-engine regression tests for the descriptive statistics builtins:
// `median`, `quantile`, `stdev`, and `variance`.
//
// All four are sample statistics: variance and stdev divide by N - 1 so
// they match R's `var` / `sd` and NumPy with `ddof=1`. Quantile uses
// linear interpolation between adjacent sorted values at position
// p * (n - 1), with p clamped to [0, 1].

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn engines() -> &'static [&'static str] {
    // Tree-walker and register VM cover the same builtin code paths via
    // shared helpers; the cranelift JIT calls into the same Rust helpers,
    // so its behavior is exercised through the VM path too.
    &["--run-tree", "--run-vm"]
}

fn run_ok(engine: &str, src: &str, fn_name: &str) -> String {
    let out = ilo()
        .args([src, engine, fn_name])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(engine: &str, src: &str, fn_name: &str) -> String {
    let out = ilo()
        .args([src, engine, fn_name])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} unexpectedly succeeded for `{src}`: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn first_line_as_f64(s: &str) -> f64 {
    s.lines()
        .next()
        .unwrap_or("")
        .trim()
        .parse::<f64>()
        .unwrap_or_else(|_| panic!("expected a number on the first stdout line, got: {s:?}"))
}

#[test]
fn median_odd_length() {
    let src = "f>n;median [1, 2, 3, 4, 5]";
    for engine in engines() {
        let out = run_ok(engine, src, "f");
        let got = first_line_as_f64(&out);
        assert!((got - 3.0).abs() < 1e-12, "engine={engine}: got {got}");
    }
}

#[test]
fn median_even_length_averages_middle_pair() {
    let src = "f>n;median [1, 2, 3, 4]";
    for engine in engines() {
        let out = run_ok(engine, src, "f");
        let got = first_line_as_f64(&out);
        assert!((got - 2.5).abs() < 1e-12, "engine={engine}: got {got}");
    }
}

#[test]
fn median_single_element() {
    let src = "f>n;median [42]";
    for engine in engines() {
        let out = run_ok(engine, src, "f");
        let got = first_line_as_f64(&out);
        assert!((got - 42.0).abs() < 1e-12, "engine={engine}: got {got}");
    }
}

#[test]
fn median_unsorted_input() {
    let src = "f>n;median [5, 1, 4, 2, 3]";
    for engine in engines() {
        let out = run_ok(engine, src, "f");
        let got = first_line_as_f64(&out);
        assert!((got - 3.0).abs() < 1e-12, "engine={engine}: got {got}");
    }
}

#[test]
fn median_empty_list_errors() {
    let src = "f>n;median []";
    for engine in engines() {
        let err = run_err(engine, src, "f");
        assert!(
            err.contains("median") || err.contains("empty"),
            "engine={engine}: stderr={err}"
        );
    }
}

#[test]
fn quantile_p50_matches_median() {
    let src = "f>n;quantile [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] 0.5";
    for engine in engines() {
        let out = run_ok(engine, src, "f");
        let got = first_line_as_f64(&out);
        // pos = 0.5 * 9 = 4.5, lerp(nums[4], nums[5]) = lerp(5, 6, 0.5) = 5.5
        assert!((got - 5.5).abs() < 1e-12, "engine={engine}: got {got}");
    }
}

#[test]
fn quantile_p90_linear_interpolation() {
    let src = "f>n;quantile [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] 0.9";
    for engine in engines() {
        let out = run_ok(engine, src, "f");
        let got = first_line_as_f64(&out);
        // pos = 0.9 * 9 = 8.1, lerp(nums[8], nums[9]) = lerp(9, 10, 0.1) = 9.1
        assert!((got - 9.1).abs() < 1e-12, "engine={engine}: got {got}");
    }
}

#[test]
fn quantile_p0_and_p1_are_endpoints() {
    let src_lo = "f>n;quantile [4, 1, 3, 2] 0";
    let src_hi = "f>n;quantile [4, 1, 3, 2] 1";
    for engine in engines() {
        let lo = first_line_as_f64(&run_ok(engine, src_lo, "f"));
        let hi = first_line_as_f64(&run_ok(engine, src_hi, "f"));
        assert!((lo - 1.0).abs() < 1e-12, "engine={engine}: got {lo}");
        assert!((hi - 4.0).abs() < 1e-12, "engine={engine}: got {hi}");
    }
}

#[test]
fn quantile_clamps_p_out_of_range() {
    // p > 1 clamps to 1 (top of range); p < 0 clamps to 0 (bottom).
    let src_above = "f>n;quantile [1, 2, 3, 4] 1.5";
    let src_below = "f>n;quantile [1, 2, 3, 4] (0 - 0.5)";
    for engine in engines() {
        let above = first_line_as_f64(&run_ok(engine, src_above, "f"));
        let below = first_line_as_f64(&run_ok(engine, src_below, "f"));
        assert!((above - 4.0).abs() < 1e-12, "engine={engine}: got {above}");
        assert!((below - 1.0).abs() < 1e-12, "engine={engine}: got {below}");
    }
}

#[test]
fn quantile_empty_list_errors() {
    let src = "f>n;quantile [] 0.5";
    for engine in engines() {
        let err = run_err(engine, src, "f");
        assert!(
            err.contains("quantile") || err.contains("empty"),
            "engine={engine}: stderr={err}"
        );
    }
}

#[test]
fn stdev_known_dataset() {
    // {2,4,4,4,5,5,7,9}: mean 5, sum of squared deviations 32, sample
    // variance 32/7, sample stdev sqrt(32/7) ≈ 2.138089935299395.
    let src = "f>n;stdev [2, 4, 4, 4, 5, 5, 7, 9]";
    for engine in engines() {
        let out = run_ok(engine, src, "f");
        let got = first_line_as_f64(&out);
        let expected = (32.0_f64 / 7.0).sqrt();
        assert!(
            (got - expected).abs() < 1e-12,
            "engine={engine}: got {got}, want {expected}"
        );
    }
}

#[test]
fn variance_known_dataset() {
    let src = "f>n;variance [2, 4, 4, 4, 5, 5, 7, 9]";
    for engine in engines() {
        let out = run_ok(engine, src, "f");
        let got = first_line_as_f64(&out);
        let expected = 32.0_f64 / 7.0;
        assert!(
            (got - expected).abs() < 1e-12,
            "engine={engine}: got {got}, want {expected}"
        );
    }
}

#[test]
fn stdev_squared_equals_variance() {
    // The two reducers must agree numerically: stdev^2 == variance.
    let src_s = "f>n;stdev [2, 4, 4, 4, 5, 5, 7, 9]";
    let src_v = "f>n;variance [2, 4, 4, 4, 5, 5, 7, 9]";
    for engine in engines() {
        let s = first_line_as_f64(&run_ok(engine, src_s, "f"));
        let v = first_line_as_f64(&run_ok(engine, src_v, "f"));
        assert!(
            (s * s - v).abs() < 1e-12,
            "engine={engine}: stdev^2={} variance={}",
            s * s,
            v
        );
    }
}

#[test]
fn single_element_variance_and_stdev_error() {
    // Sample variance/stdev divide by N - 1; for N = 1 that's a divide by
    // zero and the result is mathematically undefined. Surface a runtime
    // error rather than silently returning 0.
    for builtin in ["variance", "stdev"] {
        let src = format!("f>n;{builtin} [42]");
        for engine in engines() {
            let err = run_err(engine, &src, "f");
            assert!(
                err.contains(builtin) && err.contains("2 samples"),
                "engine={engine} {builtin}: stderr={err}"
            );
        }
    }
}

#[test]
fn nan_propagates_through_stats() {
    // Any NaN element → NaN result. Avoids silently sorting NaNs to an
    // arbitrary position via `partial_cmp(...).unwrap_or(Equal)`.
    // Build NaN at runtime as `0 / 0` to keep the source ascii-only.
    for builtin in ["median", "stdev", "variance"] {
        let src = format!("f>n;x=0/0;{builtin} [1, 2, x, 4]");
        for engine in engines() {
            let out = run_ok(engine, &src, "f");
            let got = first_line_as_f64(&out);
            assert!(
                got.is_nan(),
                "engine={engine} {builtin}: expected NaN, got {got}"
            );
        }
    }
    // quantile takes two args (xs, p).
    let qsrc = "f>n;x=0/0;quantile [1, 2, x, 4] 0.5";
    for engine in engines() {
        let out = run_ok(engine, qsrc, "f");
        let got = first_line_as_f64(&out);
        assert!(got.is_nan(), "engine={engine} quantile: got {got}");
    }
}

#[test]
fn stdev_empty_list_errors() {
    let src = "f>n;stdev []";
    for engine in engines() {
        let err = run_err(engine, src, "f");
        assert!(
            err.contains("stdev") || err.contains("empty"),
            "engine={engine}: stderr={err}"
        );
    }
}

#[test]
fn variance_empty_list_errors() {
    let src = "f>n;variance []";
    for engine in engines() {
        let err = run_err(engine, src, "f");
        assert!(
            err.contains("variance") || err.contains("empty"),
            "engine={engine}: stderr={err}"
        );
    }
}
