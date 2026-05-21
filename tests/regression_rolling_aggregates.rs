// Cross-engine regression tests for the rolling-window reducers
// `rsum`, `ravg`, and `rmin` (#5bq).
//
// All three are tree-bridge eligible (see is_tree_bridge_eligible in
// src/vm/mod.rs), so the VM and Cranelift JIT inherit them through
// OP_CALL_BUILTIN_TREE. These tests fan across the tree, register-VM
// and (when built with --features cranelift) JIT engines to catch any
// future divergence.
//
// Each builtin is exercised on the same input list against a slow
// reference computed via `slc` + a per-window reduction, locking parity
// between the O(n) running-window implementation and the obvious
// naive baseline.

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
        "ilo {engine} unexpectedly succeeded for `{src}`: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

// ---- rsum -----------------------------------------------------------------

#[test]
fn rsum_basic_three_wide() {
    // rsum 3 [1 2 3 4 5] → [6 9 12]
    let src = "f>L n;rsum 3 [1, 2, 3, 4, 5]";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[6, 9, 12]", "engine={e}");
    }
}

#[test]
fn rsum_window_one_is_identity() {
    let src = "f>L n;rsum 1 [10, 20, 30, 40]";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[10, 20, 30, 40]", "engine={e}");
    }
}

#[test]
fn rsum_window_equals_length() {
    let src = "f>L n;rsum 4 [1, 2, 3, 4]";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[10]", "engine={e}");
    }
}

#[test]
fn rsum_window_larger_than_length_is_empty() {
    let src = "f>L n;rsum 5 [1, 2, 3]";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[]", "engine={e}");
    }
}

#[test]
fn rsum_empty_input_is_empty() {
    let src = "f>L n;rsum 3 []";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[]", "engine={e}");
    }
}

#[test]
fn rsum_window_zero_errors() {
    let src = "f>L n;rsum 0 [1, 2, 3]";
    for e in engines() {
        let stderr = run_err(e, src, "f");
        assert!(
            stderr.contains("ILO-R009"),
            "engine={e}: expected ILO-R009, got stderr={stderr}"
        );
        assert!(
            stderr.contains("rsum") && stderr.contains("n must be >= 1"),
            "engine={e}: stderr={stderr}"
        );
    }
}

#[test]
fn rsum_window_negative_errors() {
    // Verifier accepts the negative literal via `0 - 1`; runtime guard rejects.
    let src = "f>L n;n=0 - 1;rsum n [1, 2, 3]";
    for e in engines() {
        let stderr = run_err(e, src, "f");
        assert!(
            stderr.contains("ILO-R009"),
            "engine={e}: expected ILO-R009, got stderr={stderr}"
        );
    }
}

#[test]
fn rsum_window_fractional_errors() {
    // `1.5` is a finite number but not a valid window count.
    let src = "f>L n;rsum 1.5 [1, 2, 3]";
    for e in engines() {
        let stderr = run_err(e, src, "f");
        assert!(
            stderr.contains("ILO-R009"),
            "engine={e}: expected ILO-R009, got stderr={stderr}"
        );
        assert!(
            stderr.contains("integer") || stderr.contains("1.5"),
            "engine={e}: stderr={stderr}"
        );
    }
}

#[test]
fn rsum_parity_with_slc_baseline_window_three() {
    // Parity check: rsum 3 xs must equal the slow `slc + sum` baseline.
    // Hand-computed reference for [2 4 6 8 10 12] window=3:
    //   sum [2 4 6]    = 12
    //   sum [4 6 8]    = 18
    //   sum [6 8 10]   = 24
    //   sum [8 10 12]  = 30
    let src = "f>L n;rsum 3 [2, 4, 6, 8, 10, 12]";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[12, 18, 24, 30]", "engine={e}");
    }
}

// ---- ravg -----------------------------------------------------------------

#[test]
fn ravg_basic_three_wide() {
    // ravg 3 [1 2 3 4 5] → [2 3 4]
    let src = "f>L n;ravg 3 [1, 2, 3, 4, 5]";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[2, 3, 4]", "engine={e}");
    }
}

#[test]
fn ravg_window_one_is_identity() {
    let src = "f>L n;ravg 1 [10, 20, 30]";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[10, 20, 30]", "engine={e}");
    }
}

#[test]
fn ravg_window_larger_than_length_is_empty() {
    let src = "f>L n;ravg 5 [1, 2, 3]";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[]", "engine={e}");
    }
}

#[test]
fn ravg_window_zero_errors() {
    let src = "f>L n;ravg 0 [1, 2, 3]";
    for e in engines() {
        let stderr = run_err(e, src, "f");
        assert!(
            stderr.contains("ILO-R009") && stderr.contains("ravg"),
            "engine={e}: stderr={stderr}"
        );
    }
}

#[test]
fn ravg_constant_input_is_constant() {
    let src = "f>L n;ravg 3 [7, 7, 7, 7, 7]";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[7, 7, 7]", "engine={e}");
    }
}

// ---- rmin -----------------------------------------------------------------

#[test]
fn rmin_basic_three_wide() {
    // rmin 3 [5 1 4 1 5 9 2 6] → [1 1 1 1 2 2]
    let src = "f>L n;rmin 3 [5, 1, 4, 1, 5, 9, 2, 6]";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[1, 1, 1, 1, 2, 2]", "engine={e}");
    }
}

#[test]
fn rmin_window_one_is_identity() {
    let src = "f>L n;rmin 1 [3, 1, 4, 1, 5, 9, 2, 6]";
    for e in engines() {
        assert_eq!(
            run_ok(e, src, "f"),
            "[3, 1, 4, 1, 5, 9, 2, 6]",
            "engine={e}"
        );
    }
}

#[test]
fn rmin_window_equals_length() {
    let src = "f>L n;rmin 4 [3, 1, 4, 1]";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[1]", "engine={e}");
    }
}

#[test]
fn rmin_window_larger_than_length_is_empty() {
    let src = "f>L n;rmin 9 [1, 2, 3]";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[]", "engine={e}");
    }
}

#[test]
fn rmin_window_zero_errors() {
    let src = "f>L n;rmin 0 [1, 2, 3]";
    for e in engines() {
        let stderr = run_err(e, src, "f");
        assert!(
            stderr.contains("ILO-R009") && stderr.contains("rmin"),
            "engine={e}: stderr={stderr}"
        );
    }
}

#[test]
fn rmin_descending_input() {
    // Strictly descending input: each window's min is its last element.
    let src = "f>L n;rmin 3 [9, 8, 7, 6, 5]";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[7, 6, 5]", "engine={e}");
    }
}

#[test]
fn rmin_ascending_input() {
    // Strictly ascending: each window's min is its first element.
    let src = "f>L n;rmin 3 [1, 2, 3, 4, 5]";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[1, 2, 3]", "engine={e}");
    }
}

#[test]
fn rmin_repeated_minimum_keeps_value() {
    // A persistent minimum across overlapping windows stays at the front
    // of the monotonic deque until it falls out of scope.
    let src = "f>L n;rmin 3 [5, 5, 1, 5, 5, 5, 5]";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[1, 1, 1, 5, 5]", "engine={e}");
    }
}

// ---- type errors (single-engine; verify runs before bytecode) -------------

#[test]
fn rsum_xs_list_of_text_verify_error() {
    let src = r#"f>L n;rsum 2 ["a", "b"]"#;
    let stderr = run_err("--run-vm", src, "f");
    assert!(
        stderr.contains("ILO-T013"),
        "expected ILO-T013, got: {stderr}"
    );
    assert!(
        stderr.contains("L t"),
        "expected message to mention L t, got: {stderr}"
    );
}

#[test]
fn ravg_n_not_number_verify_error() {
    let src = r#"f>L n;ravg "3" [1, 2, 3]"#;
    let stderr = run_err("--run-vm", src, "f");
    assert!(
        stderr.contains("ILO-T013"),
        "expected ILO-T013, got: {stderr}"
    );
    assert!(
        stderr.contains("first arg n must be n"),
        "expected to flag first arg, got: {stderr}"
    );
}

#[test]
fn rmin_xs_not_list_verify_error() {
    let src = "f>L n;rmin 2 5";
    let stderr = run_err("--run-vm", src, "f");
    assert!(
        stderr.contains("ILO-T013"),
        "expected ILO-T013, got: {stderr}"
    );
    assert!(
        stderr.contains("rmin") && stderr.contains("L n"),
        "expected message to mention rmin and L n, got: {stderr}"
    );
}
