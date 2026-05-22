// Cross-engine regression tests for the signal/math builtin cluster (0.13.0):
// `convolve`, `searchsorted`, `cabs`, `cmul`, `pairwise`, `pdist2`.
//
// Each test fans across the tree-walker and register VM (and JIT when built
// with --features cranelift) to catch any future engine divergence.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn engines() -> Vec<&'static str> {
    let mut v = vec!["tree", "--run-vm"];
    if cfg!(feature = "cranelift") {
        v.push("--jit");
    }
    v
}

fn run_ok(engine: &str, src: &str, fn_name: &str) -> String {
    let out = match engine {
        "tree" => ilo()
            .args(["run", src, fn_name])
            .output()
            .expect("failed to run ilo"),
        _ => ilo()
            .args([src, engine, fn_name])
            .output()
            .expect("failed to run ilo"),
    };
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}` fn={fn_name}: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(engine: &str, src: &str, fn_name: &str) -> String {
    let out = match engine {
        "tree" => ilo()
            .args(["run", src, fn_name])
            .output()
            .expect("failed to run ilo"),
        _ => ilo()
            .args([src, engine, fn_name])
            .output()
            .expect("failed to run ilo"),
    };
    assert!(
        !out.status.success(),
        "expected failure for `{src}` fn={fn_name} on engine={engine}"
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn parse_num(s: &str) -> f64 {
    s.lines()
        .next()
        .unwrap_or("")
        .trim()
        .parse::<f64>()
        .unwrap_or_else(|_| panic!("expected number, got: {s:?}"))
}

fn parse_list(s: &str) -> Vec<f64> {
    // ilo prints lists like: [1, 2, 3]
    let inner = s.trim().trim_start_matches('[').trim_end_matches(']');
    if inner.trim().is_empty() {
        return vec![];
    }
    inner
        .split(',')
        .map(|t| {
            t.trim()
                .parse::<f64>()
                .unwrap_or_else(|_| panic!("expected number in list, got: {t:?}"))
        })
        .collect()
}

// ── convolve ─────────────────────────────────────────────────────────────────

#[test]
fn convolve_basic() {
    // convolve [1,2,3] [0,1,0.5] → [0, 1, 2.5, 4, 1.5]
    // i.e. polynomial [1 2 3] * [0 1 0.5]
    for engine in engines() {
        let src = "f>L n;convolve [1,2,3] [0,1,0.5]";
        let out = run_ok(engine, src, "f");
        let got = parse_list(&out);
        let want = [0.0, 1.0, 2.5, 4.0, 1.5];
        assert_eq!(got.len(), want.len(), "engine={engine}: length mismatch");
        for (g, w) in got.iter().zip(want.iter()) {
            assert!((g - w).abs() < 1e-9, "engine={engine}: got {g} want {w}");
        }
    }
}

#[test]
fn convolve_single_element_each() {
    // convolve [3] [4] → [12]
    for engine in engines() {
        let src = "f>L n;convolve [3] [4]";
        let out = run_ok(engine, src, "f");
        let got = parse_list(&out);
        assert_eq!(got.len(), 1, "engine={engine}");
        assert!((got[0] - 12.0).abs() < 1e-9, "engine={engine}");
    }
}

#[test]
fn convolve_identity_kernel() {
    // convolving with [1] is a no-op
    for engine in engines() {
        let src = "f>L n;convolve [1,2,3,4] [1]";
        let out = run_ok(engine, src, "f");
        let got = parse_list(&out);
        let want = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(got.len(), want.len(), "engine={engine}");
        for (g, w) in got.iter().zip(want.iter()) {
            assert!((g - w).abs() < 1e-9, "engine={engine}: got {g} want {w}");
        }
    }
}

#[test]
fn convolve_empty_first_arg_errors() {
    for engine in engines() {
        let src = "f>L n;convolve [] [1,2]";
        let err = run_err(engine, src, "f");
        assert!(err.contains("ILO-R009"), "engine={engine}: stderr={err}");
    }
}

// ── searchsorted ─────────────────────────────────────────────────────────────

#[test]
fn searchsorted_basic() {
    // searchsorted [1,3,5,7] [0,2,4,6,8] → [0,1,2,3,4]
    for engine in engines() {
        let src = "f>L n;searchsorted [1,3,5,7] [0,2,4,6,8]";
        let out = run_ok(engine, src, "f");
        let got = parse_list(&out);
        let want = [0.0, 1.0, 2.0, 3.0, 4.0];
        assert_eq!(got.len(), want.len(), "engine={engine}");
        for (g, w) in got.iter().zip(want.iter()) {
            assert!((g - w).abs() < 1e-9, "engine={engine}");
        }
    }
}

#[test]
fn searchsorted_duplicates_leftmost() {
    // bisect_left: equal element → leftmost index
    for engine in engines() {
        let src = "f>L n;searchsorted [1,2,2,2,3] [2]";
        let out = run_ok(engine, src, "f");
        let got = parse_list(&out);
        assert!((got[0] - 1.0).abs() < 1e-9, "engine={engine}: got {:?}", got);
    }
}

#[test]
fn searchsorted_empty_targets_returns_empty() {
    for engine in engines() {
        let src = "f>L n;searchsorted [1,2,3] []";
        let out = run_ok(engine, src, "f");
        let got = parse_list(&out);
        assert!(got.is_empty(), "engine={engine}: got {:?}", got);
    }
}

// ── cabs ─────────────────────────────────────────────────────────────────────

#[test]
fn cabs_3_4_gives_5() {
    // |3 + 4i| = 5
    for engine in engines() {
        let src = "f>n;cabs [3,4]";
        let out = run_ok(engine, src, "f");
        let got = parse_num(&out);
        assert!((got - 5.0).abs() < 1e-9, "engine={engine}: got {got}");
    }
}

#[test]
fn cabs_zero_pair() {
    for engine in engines() {
        let src = "f>n;cabs [0,0]";
        let out = run_ok(engine, src, "f");
        let got = parse_num(&out);
        assert!((got - 0.0).abs() < 1e-9, "engine={engine}");
    }
}

#[test]
fn cabs_pure_real() {
    for engine in engines() {
        let src = "f>n;cabs [7,0]";
        let out = run_ok(engine, src, "f");
        let got = parse_num(&out);
        assert!((got - 7.0).abs() < 1e-9, "engine={engine}");
    }
}

#[test]
fn cabs_wrong_length_errors() {
    for engine in engines() {
        let src = "f>n;cabs [1,2,3]";
        let err = run_err(engine, src, "f");
        assert!(err.contains("ILO-R009"), "engine={engine}: stderr={err}");
    }
}

// ── cmul ─────────────────────────────────────────────────────────────────────

#[test]
fn cmul_basic() {
    // (1+2i) * (3+4i) = (1*3-2*4) + (1*4+2*3)i = -5 + 10i
    for engine in engines() {
        let src = "f>L n;cmul [1,2] [3,4]";
        let out = run_ok(engine, src, "f");
        let got = parse_list(&out);
        assert_eq!(got.len(), 2, "engine={engine}");
        assert!((got[0] - (-5.0)).abs() < 1e-9, "engine={engine}: re={}", got[0]);
        assert!((got[1] - 10.0).abs() < 1e-9, "engine={engine}: im={}", got[1]);
    }
}

#[test]
fn cmul_by_zero() {
    for engine in engines() {
        let src = "f>L n;cmul [3,4] [0,0]";
        let out = run_ok(engine, src, "f");
        let got = parse_list(&out);
        assert_eq!(got.len(), 2, "engine={engine}");
        assert!((got[0] - 0.0).abs() < 1e-9, "engine={engine}");
        assert!((got[1] - 0.0).abs() < 1e-9, "engine={engine}");
    }
}

#[test]
fn cmul_by_one() {
    // multiply by [1,0] (real 1) should be identity
    for engine in engines() {
        let src = "f>L n;cmul [5,7] [1,0]";
        let out = run_ok(engine, src, "f");
        let got = parse_list(&out);
        assert!((got[0] - 5.0).abs() < 1e-9, "engine={engine}: re={}", got[0]);
        assert!((got[1] - 7.0).abs() < 1e-9, "engine={engine}: im={}", got[1]);
    }
}

// ── pairwise ─────────────────────────────────────────────────────────────────

#[test]
fn pairwise_differences() {
    // finite differences: pairwise (a:n b:n>n; - b a) [1,3,6,10] → [2,3,4]
    for engine in engines() {
        let src = "diff a:n b:n>n;- b a\nf>L n;pairwise diff [1,3,6,10]";
        let out = run_ok(engine, src, "f");
        let got = parse_list(&out);
        let want = [2.0, 3.0, 4.0];
        assert_eq!(got.len(), want.len(), "engine={engine}");
        for (g, w) in got.iter().zip(want.iter()) {
            assert!((g - w).abs() < 1e-9, "engine={engine}");
        }
    }
}

#[test]
fn pairwise_empty_list_returns_empty() {
    for engine in engines() {
        let src = "add a:n b:n>n;+ a b\nf>L n;pairwise add []";
        let out = run_ok(engine, src, "f");
        let got = parse_list(&out);
        assert!(got.is_empty(), "engine={engine}: got {:?}", got);
    }
}

#[test]
fn pairwise_singleton_returns_empty() {
    for engine in engines() {
        let src = "add a:n b:n>n;+ a b\nf>L n;pairwise add [5]";
        let out = run_ok(engine, src, "f");
        let got = parse_list(&out);
        assert!(got.is_empty(), "engine={engine}: got {:?}", got);
    }
}

// ── pdist2 ───────────────────────────────────────────────────────────────────

#[test]
fn pdist2_basic() {
    // pdist2 [0,3,1] [0,0,4] → [0, 9, 9]
    for engine in engines() {
        let src = "f>L n;pdist2 [0,3,1] [0,0,4]";
        let out = run_ok(engine, src, "f");
        let got = parse_list(&out);
        let want = [0.0, 9.0, 9.0];
        assert_eq!(got.len(), want.len(), "engine={engine}");
        for (g, w) in got.iter().zip(want.iter()) {
            assert!((g - w).abs() < 1e-9, "engine={engine}");
        }
    }
}

#[test]
fn pdist2_identical_lists_all_zeros() {
    for engine in engines() {
        let src = "f>L n;pdist2 [1,2,3] [1,2,3]";
        let out = run_ok(engine, src, "f");
        let got = parse_list(&out);
        for g in &got {
            assert!((g - 0.0).abs() < 1e-9, "engine={engine}: got {g}");
        }
    }
}

#[test]
fn pdist2_length_mismatch_errors() {
    for engine in engines() {
        let src = "f>L n;pdist2 [1,2] [1,2,3]";
        let err = run_err(engine, src, "f");
        assert!(err.contains("ILO-R009"), "engine={engine}: stderr={err}");
    }
}
