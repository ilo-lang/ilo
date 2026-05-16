// Cross-engine regression tests for `sum` and `avg`.
//
// Before this fix, the VM compiler had no dispatch arm for `Builtin::Sum`
// or `Builtin::Avg`, so any call fell through to the named-function lookup
// and failed with "Compile error: undefined function: sum" (and likewise
// for `avg`) on both `--run-vm` and `--run-cranelift`. The tree-walking
// interpreter handled both builtins directly and worked correctly.
//
// These tests pin the behaviour across all three engines: happy paths,
// the `avg []` error case, the `sum []` zero case, and type/argument
// errors (non-list, non-numeric elements).

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_ok(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} {src:?} unexpectedly failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} {src:?} unexpectedly succeeded: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    // ilo emits structured JSON diagnostics on stderr; we just check for
    // substrings so the test is robust to formatting changes.
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm"];

// ── sum ──────────────────────────────────────────────────────────────

#[test]
fn sum_happy_path_cross_engine() {
    let src = "f>n;sum [1, 2, 3, 4]";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "f"), "10", "{engine}: sum [1..4] = 10");
    }
}

#[test]
fn sum_floats_cross_engine() {
    let src = "f>n;sum [1.5, 2.5, 3.0]";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "f"), "7", "{engine}: sum floats");
    }
}

#[test]
fn sum_singleton_cross_engine() {
    let src = "f>n;sum [42]";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "f"), "42", "{engine}: sum [42]");
    }
}

#[test]
fn sum_empty_returns_zero_cross_engine() {
    // Tree-walker semantics: `sum []` is 0, not an error. VM and Cranelift
    // must match.
    let src = "f>n;sum []";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "f"), "0", "{engine}: sum [] = 0");
    }
}

#[test]
fn sum_negative_numbers_cross_engine() {
    let src = "f>n;sum [-1, -2, -3]";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "f"), "-6", "{engine}: sum negatives");
    }
}

#[test]
fn sum_non_list_errors_cross_engine() {
    let src = "f>n;sum 42";
    for engine in ENGINES_ALL {
        let err = run_err(engine, src, "f");
        assert!(
            err.contains("sum") || err.contains("list"),
            "{engine}: expected sum/list in error, got: {err}"
        );
    }
}

#[test]
fn sum_non_numeric_element_errors_cross_engine() {
    let src = r#"f>n;sum ["a", "b"]"#;
    for engine in ENGINES_ALL {
        let err = run_err(engine, src, "f");
        assert!(
            err.contains("sum") || err.contains("number"),
            "{engine}: expected sum/number in error, got: {err}"
        );
    }
}

// ── avg ──────────────────────────────────────────────────────────────

#[test]
fn avg_happy_path_cross_engine() {
    let src = "f>n;avg [1, 2, 3, 4]";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "f"), "2.5", "{engine}: avg [1..4]");
    }
}

#[test]
fn avg_integer_result_cross_engine() {
    let src = "f>n;avg [2, 4, 6]";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "f"), "4", "{engine}: avg [2,4,6] = 4");
    }
}

#[test]
fn avg_singleton_cross_engine() {
    let src = "f>n;avg [42]";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "f"), "42", "{engine}: avg [42]");
    }
}

#[test]
fn avg_negative_numbers_cross_engine() {
    let src = "f>n;avg [-2, -4, -6]";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "f"), "-4", "{engine}: avg negatives");
    }
}

#[test]
fn avg_empty_errors_cross_engine() {
    // Tree-walker semantics: `avg []` is an error (cannot average an empty
    // list). VM and Cranelift must error too.
    let src = "f>n;avg []";
    for engine in ENGINES_ALL {
        let err = run_err(engine, src, "f");
        assert!(
            err.contains("avg") && (err.contains("empty") || err.contains("average")),
            "{engine}: expected avg/empty in error, got: {err}"
        );
    }
}

#[test]
fn avg_non_list_errors_cross_engine() {
    let src = "f>n;avg 42";
    for engine in ENGINES_ALL {
        let err = run_err(engine, src, "f");
        assert!(
            err.contains("avg") || err.contains("list"),
            "{engine}: expected avg/list in error, got: {err}"
        );
    }
}

#[test]
fn avg_non_numeric_element_errors_cross_engine() {
    let src = r#"f>n;avg ["a", "b"]"#;
    for engine in ENGINES_ALL {
        let err = run_err(engine, src, "f");
        assert!(
            err.contains("avg") || err.contains("number"),
            "{engine}: expected avg/number in error, got: {err}"
        );
    }
}

// ── interactions ──────────────────────────────────────────────────────

#[test]
fn sum_avg_composed_cross_engine() {
    // Exercises both opcodes in a single function body to make sure they
    // can coexist in the same compiled chunk on every engine.
    let src = "f>n;-(avg [10, 20, 30]) (sum [1, 2, 3])";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "f"), "14", "{engine}: avg - sum");
    }
}

#[test]
fn sum_avg_arithmetic_on_results_cross_engine() {
    // Regression: the Cranelift JIT keeps an F64 shadow per register for
    // OP_*_NN fast paths. Helper-call opcodes (OP_SUM, OP_AVG, and the
    // adjacent OP_MEDIAN/OP_STDEV/OP_VARIANCE/OP_QUANTILE) used to write
    // only the I64 NanVal slot, leaving the F64 shadow stale at zero.
    // A subsequent OP_SUB_NN/OP_ADD_NN over the result then read the
    // stale shadow and silently produced 0.
    let src = "f>n;a=avg [10, 20, 30];s=sum [1, 2, 3];-a s";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "f"), "14", "{engine}: -avg sum");
    }
    let src = "f>n;a=avg [10, 20, 30];s=sum [1, 2, 3];+a s";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "f"), "26", "{engine}: +avg sum");
    }
    // Same hazard for the adjacent stats family; covering median here keeps
    // the F64-shadow contract consistent across every helper-call numeric op.
    let src = "f>n;a=median [10, 20, 30];b=median [1, 2, 3];-a b";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "f"), "18", "{engine}: median diff");
    }
}
