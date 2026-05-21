// Cross-engine regression tests for `linspace`, `ones`, `rep` — the numeric
// prelude builtins added in 0.12.1.
//
// Before this change three persona classes converged on hand-rolled list
// constructors: linear-regression needed `linspace` for evenly-spaced sample
// points (manual fld + step computation), distance-matrix needed `ones` for
// a design-matrix column (manual `map (i>1) (range 0 n)`), and monte-carlo
// needed `rep` for seeding accumulators (same map pattern). The new builtins
// save ~5-10 LoC per call site.
//
// All three lower through `is_tree_bridge_eligible`, so the bridge is the
// same path on every engine; the test fans across tree / register VM / JIT
// (when built with --features cranelift) anyway to catch any future drift.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn engines() -> Vec<&'static str> {
    let mut v = vec!["tree", "--vm"];
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
        "ilo {engine} unexpectedly succeeded for `{src}`: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

// ---------- linspace ----------

#[test]
fn linspace_basic_inclusive() {
    let src = "f>L n;linspace 0 10 5";
    for e in engines() {
        let got = run_ok(e, src, "f");
        assert_eq!(got, "[0, 2.5, 5, 7.5, 10]", "engine={e}");
    }
}

#[test]
fn linspace_n_one_returns_a() {
    // numpy endpoint=True convention: a single point returns [a].
    let src = "f>L n;linspace 0 10 1";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[0]", "engine={e}");
    }
}

#[test]
fn linspace_n_zero_returns_empty() {
    let src = "f>L n;linspace 0 10 0";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[]", "engine={e}");
    }
}

#[test]
fn linspace_equal_endpoints() {
    // a == b means every element is a regardless of n.
    let src = "f>L n;linspace 5 5 3";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[5, 5, 5]", "engine={e}");
    }
}

#[test]
fn linspace_negative_range() {
    let src = "f>L n;linspace 10 0 5";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[10, 7.5, 5, 2.5, 0]", "engine={e}");
    }
}

#[test]
fn linspace_endpoint_pinned_exact() {
    // Last element must equal b exactly to avoid float-accumulated drift.
    // (1.0 / 3.0) * 3 != 1.0 without the explicit endpoint pin.
    let src = "f>n;at (linspace 0 1 4) -1";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "1", "engine={e}");
    }
}

#[test]
fn linspace_rejects_negative_n() {
    let src = "f>L n;linspace 0 10 -1";
    for e in engines() {
        let err = run_err(e, src, "f");
        assert!(
            err.contains("linspace"),
            "engine={e} err missing 'linspace': {err}"
        );
    }
}

#[test]
fn linspace_rejects_fractional_n() {
    // n must be an integer: fract != 0 hits the same ILO-R009 path as negative-n.
    let src = "f>L n;linspace 0 10 2.5";
    for e in engines() {
        let err = run_err(e, src, "f");
        assert!(
            err.contains("non-negative integer"),
            "engine={e} err missing 'non-negative integer': {err}"
        );
    }
}

#[test]
fn linspace_rejects_over_cap() {
    // Cap is 1_000_000 elements. 1_000_001 must reject; we exercise this
    // instead of the boundary itself because cargo test on linspace 0 1 1000000
    // would allocate a 1M-element list per engine just to throw it away.
    let src = "f>L n;linspace 0 1 1000001";
    for e in engines() {
        let err = run_err(e, src, "f");
        assert!(
            err.contains("too large"),
            "engine={e} err missing 'too large': {err}"
        );
    }
}

// ---------- ones ----------

#[test]
fn ones_basic() {
    let src = "f>L n;ones 5";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[1, 1, 1, 1, 1]", "engine={e}");
    }
}

#[test]
fn ones_zero_returns_empty() {
    let src = "f>L n;ones 0";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[]", "engine={e}");
    }
}

#[test]
fn ones_one() {
    let src = "f>L n;ones 1";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[1]", "engine={e}");
    }
}

#[test]
fn ones_sums_to_n() {
    // Sanity check the design-matrix column use case: `sum (ones n)` == n.
    let src = "f>n;sum (ones 17)";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "17", "engine={e}");
    }
}

#[test]
fn ones_rejects_negative() {
    let src = "f>L n;ones -3";
    for e in engines() {
        let err = run_err(e, src, "f");
        assert!(err.contains("ones"), "engine={e} err missing 'ones': {err}");
    }
}

#[test]
fn ones_rejects_fractional() {
    let src = "f>L n;ones 2.5";
    for e in engines() {
        let err = run_err(e, src, "f");
        assert!(
            err.contains("non-negative integer"),
            "engine={e} err missing 'non-negative integer': {err}"
        );
    }
}

#[test]
fn ones_rejects_over_cap() {
    let src = "f>L n;ones 1000001";
    for e in engines() {
        let err = run_err(e, src, "f");
        assert!(
            err.contains("too large"),
            "engine={e} err missing 'too large': {err}"
        );
    }
}

// ---------- rep ----------

#[test]
fn rep_number() {
    let src = "f>L n;rep 3 7";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[7, 7, 7]", "engine={e}");
    }
}

#[test]
fn rep_text() {
    let src = "f>L t;rep 3 \"x\"";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[x, x, x]", "engine={e}");
    }
}

#[test]
fn rep_bool() {
    let src = "f>L b;rep 4 true";
    for e in engines() {
        assert_eq!(
            run_ok(e, src, "f"),
            "[true, true, true, true]",
            "engine={e}"
        );
    }
}

#[test]
fn rep_zero_returns_empty() {
    let src = "f>L n;rep 0 42";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[]", "engine={e}");
    }
}

#[test]
fn rep_one() {
    let src = "f>L n;rep 1 42";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[42]", "engine={e}");
    }
}

#[test]
fn rep_rejects_negative() {
    let src = "f>L n;rep -1 9";
    for e in engines() {
        let err = run_err(e, src, "f");
        assert!(err.contains("rep"), "engine={e} err missing 'rep': {err}");
    }
}

#[test]
fn rep_length_matches_count() {
    let src = "f>n;len (rep 12 0)";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "12", "engine={e}");
    }
}

#[test]
fn rep_rejects_fractional() {
    let src = "f>L n;rep 2.5 1";
    for e in engines() {
        let err = run_err(e, src, "f");
        assert!(
            err.contains("non-negative integer"),
            "engine={e} err missing 'non-negative integer': {err}"
        );
    }
}

#[test]
fn rep_rejects_over_cap() {
    let src = "f>L n;rep 1000001 0";
    for e in engines() {
        let err = run_err(e, src, "f");
        assert!(
            err.contains("too large"),
            "engine={e} err missing 'too large': {err}"
        );
    }
}

#[test]
fn rep_value_semantics_with_list_element() {
    // Manifesto promise: lists are value-typed. `rep` with a list element must
    // give n logically-independent copies — reading one back must return the
    // original value, not anything stamped by reading or destructuring siblings.
    // (ilo lists are persistent / Arc-shared internally, but never observable
    // as aliasing from the user's perspective.) This is the smoke test that
    // would scream if rep ever switched to a shallow Vec::clone of a Vec<Arc>
    // and lost the value-equal-but-independent guarantee.
    let src = "f>L n;xs=rep 3 [1,2,3];at xs 1";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[1, 2, 3]", "engine={e}");
    }
}
