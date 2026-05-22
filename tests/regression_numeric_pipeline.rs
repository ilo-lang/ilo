// Cross-engine regression tests for the numeric-pipeline primitives added in
// 0.13.0: `arange`, `zeros`, `vstack`, `hstack`, `column-stack`, `hist`.
//
// These builtins close the gap between ilo's linear-algebra core and the
// constructive inputs needed to feed it: OLS ~2.25x compression, histogram
// ~3x versus the hand-rolled equivalents.
//
// All six lower through `is_tree_bridge_eligible`, so the bridge is the same
// path on every engine; the test fans across tree / register VM / JIT
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

// ---------- zeros ----------

#[test]
fn zeros_basic() {
    let src = "f>L n;zeros 4";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[0, 0, 0, 0]", "engine={e}");
    }
}

#[test]
fn zeros_empty() {
    let src = "f>L n;zeros 0";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[]", "engine={e}");
    }
}

#[test]
fn zeros_sums_to_zero() {
    let src = "f>n;sum (zeros 10)";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "0", "engine={e}");
    }
}

#[test]
fn zeros_rejects_negative() {
    let src = "f>L n;zeros -1";
    for e in engines() {
        let err = run_err(e, src, "f");
        assert!(
            err.contains("zeros"),
            "engine={e} err missing 'zeros': {err}"
        );
    }
}

#[test]
fn zeros_rejects_fractional() {
    let src = "f>L n;zeros 2.5";
    for e in engines() {
        let err = run_err(e, src, "f");
        assert!(
            err.contains("non-negative integer"),
            "engine={e} err missing 'non-negative integer': {err}"
        );
    }
}

// ---------- arange ----------

#[test]
fn arange_basic() {
    let src = "f>L n;arange 0 5 1";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[0, 1, 2, 3, 4]", "engine={e}");
    }
}

#[test]
fn arange_float_step() {
    let src = "f>n;len (arange 0 1 0.1)";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "10", "engine={e}");
    }
}

#[test]
fn arange_start_ge_stop_empty() {
    let src = "f>L n;arange 5 5 1";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[]", "engine={e}");
    }
}

#[test]
fn arange_start_gt_stop_empty() {
    let src = "f>L n;arange 10 5 1";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[]", "engine={e}");
    }
}

#[test]
fn arange_rejects_nonpositive_step() {
    let src = "f>L n;arange 0 10 0";
    for e in engines() {
        let err = run_err(e, src, "f");
        assert!(
            err.contains("step must be positive"),
            "engine={e} err missing 'step must be positive': {err}"
        );
    }
}

#[test]
fn arange_rejects_negative_step() {
    let src = "f>L n;arange 0 10 -1";
    for e in engines() {
        let err = run_err(e, src, "f");
        assert!(
            err.contains("step must be positive"),
            "engine={e} err missing 'step must be positive': {err}"
        );
    }
}

// ---------- vstack ----------

#[test]
fn vstack_two_matrices() {
    // vstack [[1,2],[3,4]] [[5,6]] → 3 rows
    let src = "f>n;ms=[[1,2],[3,4],[5,6]];a=slc ms 0 2;b=slc ms 2 3;len (vstack [a,b])";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "3", "engine={e}");
    }
}

#[test]
fn vstack_single() {
    let src = "f>L n;vstack [[[1,2],[3,4]]]";
    for e in engines() {
        let got = run_ok(e, src, "f");
        assert_eq!(got, "[[1, 2], [3, 4]]", "engine={e}");
    }
}

#[test]
fn vstack_empty_outer() {
    let src = "f>L n;vstack []";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[]", "engine={e}");
    }
}

#[test]
fn vstack_rejects_non_list_element() {
    let src = "f>L n;vstack [42]";
    for e in engines() {
        let err = run_err(e, src, "f");
        assert!(
            err.contains("vstack"),
            "engine={e} err missing 'vstack': {err}"
        );
    }
}

// ---------- hstack ----------

#[test]
fn hstack_two_row_matrices() {
    // hstack [[[1,2],[3,4]], [[5],[6]]] → [[1,2,5],[3,4,6]]
    let src = "f>n;a=[[1,2],[3,4]];b=[[5],[6]];m=hstack [a,b];len m";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "2", "engine={e}");
    }
}

#[test]
fn hstack_row_width() {
    let src = "f>n;a=[[1,2],[3,4]];b=[[5],[6]];m=hstack [a,b];len (at m 0)";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "3", "engine={e}");
    }
}

#[test]
fn hstack_empty_outer() {
    let src = "f>L n;hstack []";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[]", "engine={e}");
    }
}

#[test]
fn hstack_rejects_row_count_mismatch() {
    let src = "f>L n;a=[[1],[2]];b=[[3]];hstack [a,b]";
    for e in engines() {
        let err = run_err(e, src, "f");
        assert!(
            err.contains("hstack"),
            "engine={e} err missing 'hstack': {err}"
        );
    }
}

// ---------- column-stack ----------

#[test]
fn column_stack_two_vectors() {
    // column-stack [[1,2,3],[4,5,6]] → [[1,4],[2,5],[3,6]]
    let src = "f>n;m=column-stack [[1,2,3],[4,5,6]];len m";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "3", "engine={e}");
    }
}

#[test]
fn column_stack_row_width() {
    let src = "f>n;m=column-stack [[1,2,3],[4,5,6]];len (at m 0)";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "2", "engine={e}");
    }
}

#[test]
fn column_stack_first_element() {
    let src = "f>n;m=column-stack [[1,2,3],[4,5,6]];at (at m 0) 0";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "1", "engine={e}");
    }
}

#[test]
fn column_stack_empty_outer() {
    let src = "f>L n;column-stack []";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[]", "engine={e}");
    }
}

#[test]
fn column_stack_rejects_length_mismatch() {
    let src = "f>L n;column-stack [[1,2],[3,4,5]]";
    for e in engines() {
        let err = run_err(e, src, "f");
        assert!(
            err.contains("column-stack"),
            "engine={e} err missing 'column-stack': {err}"
        );
    }
}

// ---------- hist ----------

#[test]
fn hist_basic_counts() {
    // xs=[0,1,2,3,4], 5 bins of width 1 → each bin has count 1
    let src = "f>n;sum (hist [0,1,2,3,4] 5)";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "5", "engine={e}");
    }
}

#[test]
fn hist_all_same_value() {
    // All values equal → everything lands in one bin.
    let src = "f>n;xs=rep 10 5;counts=hist xs 4;at counts 0";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "10", "engine={e}");
    }
}

#[test]
fn hist_empty_input() {
    let src = "f>n;len (hist [] 3)";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "3", "engine={e}");
    }
}

#[test]
fn hist_empty_all_zero() {
    let src = "f>n;sum (hist [] 5)";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "0", "engine={e}");
    }
}

#[test]
fn hist_bin_count() {
    let src = "f>n;len (hist [1,2,3,4,5] 3)";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "3", "engine={e}");
    }
}

#[test]
fn hist_rejects_zero_bins() {
    let src = "f>L n;hist [1,2,3] 0";
    for e in engines() {
        let err = run_err(e, src, "f");
        assert!(err.contains("hist"), "engine={e} err missing 'hist': {err}");
    }
}

#[test]
fn hist_rejects_negative_bins() {
    let src = "f>L n;hist [1,2,3] -1";
    for e in engines() {
        let err = run_err(e, src, "f");
        assert!(
            err.contains("positive integer"),
            "engine={e} err missing 'positive integer': {err}"
        );
    }
}

#[test]
fn hist_total_counts_match_input_len() {
    // Sum of histogram bins must equal len of input.
    let src = "f>n;xs=arange 0 100 1;sum (hist xs 10)";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "100", "engine={e}");
    }
}
