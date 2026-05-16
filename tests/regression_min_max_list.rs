// Cross-engine regression tests for the 1-arg list form of `min` and `max`.
//
// Before this fix, `max [1 2 3]` failed with `ILO-T006 arity mismatch: 'max'
// expects 2 args, got 1`, forcing personas to write `fld max xs init` — an
// API asymmetry with `avg`/`median`/`stdev`/`variance` which already accept
// a single list. The 2-arg numeric form `min a b` / `max a b` is preserved
// (still required by `fld min xs init` as the accumulator), so the change
// is purely additive: existing programs keep working, new programs can use
// the natural shape.
//
// Each test runs through tree and VM; cranelift lowers OP_MIN_LST/OP_MAX_LST
// into the same `vm_min_max_lst` helper, so its behaviour is exercised
// transitively through the VM path.
//
// Empty lists error per the same contract as `median`/`avg`. NaN elements
// propagate to a NaN result, matching the stats-builtins NaN policy.
// Non-number elements error with a type message naming the builtin.
// Non-list arguments (e.g. plain number) error with a list-expected message.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn engines() -> &'static [&'static str] {
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
fn min_list_basic() {
    let src = "f>n;min [3, 1, 4, 1, 5, 9, 2, 6]";
    for engine in engines() {
        let got = first_line_as_f64(&run_ok(engine, src, "f"));
        assert!((got - 1.0).abs() < 1e-12, "engine={engine}: got {got}");
    }
}

#[test]
fn max_list_basic() {
    let src = "f>n;max [3, 1, 4, 1, 5, 9, 2, 6]";
    for engine in engines() {
        let got = first_line_as_f64(&run_ok(engine, src, "f"));
        assert!((got - 9.0).abs() < 1e-12, "engine={engine}: got {got}");
    }
}

#[test]
fn min_list_single_element() {
    let src = "f>n;min [42]";
    for engine in engines() {
        let got = first_line_as_f64(&run_ok(engine, src, "f"));
        assert!((got - 42.0).abs() < 1e-12, "engine={engine}: got {got}");
    }
}

#[test]
fn max_list_single_element() {
    let src = "f>n;max [42]";
    for engine in engines() {
        let got = first_line_as_f64(&run_ok(engine, src, "f"));
        assert!((got - 42.0).abs() < 1e-12, "engine={engine}: got {got}");
    }
}

#[test]
fn min_list_negative_numbers() {
    // Negative literals must be parsed inside list, and the smallest of a
    // mixed-sign list must be the most-negative element.
    let src = "f>n;min [3, 0 - 5, 7, 0 - 12, 4]";
    for engine in engines() {
        let got = first_line_as_f64(&run_ok(engine, src, "f"));
        assert!((got + 12.0).abs() < 1e-12, "engine={engine}: got {got}");
    }
}

#[test]
fn max_list_negative_numbers() {
    let src = "f>n;max [0 - 3, 0 - 5, 0 - 1, 0 - 12]";
    for engine in engines() {
        let got = first_line_as_f64(&run_ok(engine, src, "f"));
        assert!((got + 1.0).abs() < 1e-12, "engine={engine}: got {got}");
    }
}

#[test]
fn min_list_on_bound_local() {
    // The bound-local path exercises non-literal list inputs (different
    // codegen path: register lookup vs inlined list literal).
    let src = "f>n;xs=[10, 4, 22, 7];min xs";
    for engine in engines() {
        let got = first_line_as_f64(&run_ok(engine, src, "f"));
        assert!((got - 4.0).abs() < 1e-12, "engine={engine}: got {got}");
    }
}

#[test]
fn max_list_on_bound_local() {
    let src = "f>n;xs=[10, 4, 22, 7];max xs";
    for engine in engines() {
        let got = first_line_as_f64(&run_ok(engine, src, "f"));
        assert!((got - 22.0).abs() < 1e-12, "engine={engine}: got {got}");
    }
}

#[test]
fn min_max_two_arg_form_preserved() {
    // Critical: the 2-arg numeric form must keep working unchanged. This is
    // the same form that `fld min xs init` relies on as an accumulator.
    for engine in engines() {
        let got_min = first_line_as_f64(&run_ok(engine, "f>n;min 3 7", "f"));
        let got_max = first_line_as_f64(&run_ok(engine, "f>n;max 3 7", "f"));
        assert!(
            (got_min - 3.0).abs() < 1e-12,
            "engine={engine}: min got {got_min}"
        );
        assert!(
            (got_max - 7.0).abs() < 1e-12,
            "engine={engine}: max got {got_max}"
        );
    }
}

#[test]
fn fld_min_max_still_works_as_accumulator() {
    // `min`/`max` are still valid `fld` accumulators because the 2-arg
    // `n n -> n` form is unchanged. This is the form personas had to use
    // before, and it should keep working alongside the new list form.
    for engine in engines() {
        let got_min = first_line_as_f64(&run_ok(
            engine,
            "f>n;fld min [3, 1, 4, 1, 5, 9, 2, 6] 999",
            "f",
        ));
        let got_max = first_line_as_f64(&run_ok(
            engine,
            "f>n;fld max [3, 1, 4, 1, 5, 9, 2, 6] 0",
            "f",
        ));
        assert!(
            (got_min - 1.0).abs() < 1e-12,
            "engine={engine}: got {got_min}"
        );
        assert!(
            (got_max - 9.0).abs() < 1e-12,
            "engine={engine}: got {got_max}"
        );
    }
}

#[test]
fn min_empty_list_errors() {
    let src = "f>n;min []";
    for engine in engines() {
        let err = run_err(engine, src, "f");
        assert!(
            err.contains("min") && err.contains("empty"),
            "engine={engine}: stderr={err}"
        );
    }
}

#[test]
fn max_empty_list_errors() {
    let src = "f>n;max []";
    for engine in engines() {
        let err = run_err(engine, src, "f");
        assert!(
            err.contains("max") && err.contains("empty"),
            "engine={engine}: stderr={err}"
        );
    }
}

#[test]
fn min_list_with_nan_propagates() {
    // Any NaN element → NaN result. Matches the stats-builtins contract
    // and avoids silent mis-comparison via partial_cmp .unwrap_or(Equal).
    let src = "f>n;x=sqrt -1;min [1, 2, x, 4]";
    for engine in engines() {
        let got = first_line_as_f64(&run_ok(engine, src, "f"));
        assert!(got.is_nan(), "engine={engine}: expected NaN, got {got}");
    }
}

#[test]
fn max_list_with_nan_propagates() {
    let src = "f>n;x=sqrt -1;max [1, 2, x, 4]";
    for engine in engines() {
        let got = first_line_as_f64(&run_ok(engine, src, "f"));
        assert!(got.is_nan(), "engine={engine}: expected NaN, got {got}");
    }
}

#[test]
fn min_non_list_arg_errors() {
    // Caught at verify time: `min` 1-arg form expects `L n`. A plain number
    // is the most common mistake — the verifier should reject before
    // running rather than letting the runtime produce a misleading error.
    let src = "f>n;min 42";
    for engine in engines() {
        let err = run_err(engine, src, "f");
        assert!(
            err.contains("min") && (err.contains("L n") || err.contains("list")),
            "engine={engine}: stderr={err}"
        );
    }
}
