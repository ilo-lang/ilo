// Cross-engine regression tests for `where` — parallel-list conditional select.
//
// `where cond xs ys > L a` is the NumPy `np.where` equivalent: for each i,
// `output[i] = xs[i] if cond[i] else ys[i]`. All three lists must be the same
// length; mismatch raises ILO-R009.
//
// Tree-bridge eligible (see is_tree_bridge_eligible in src/vm/mod.rs), so the
// VM and Cranelift JIT inherit it through OP_CALL_BUILTIN_TREE. The tests fan
// across tree, register VM, and (when built with --features cranelift) the
// JIT to catch any future divergence.

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

#[test]
fn where_mixed_bool_numbers() {
    // The textbook case: pick from xs where cond is true, from ys otherwise.
    let src = "f>L n;where [true, false, true] [1, 2, 3] [10, 20, 30]";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[1, 20, 3]", "engine={e}");
    }
}

#[test]
fn where_all_true_picks_xs() {
    let src = "f>L n;where [true, true, true] [1, 2, 3] [10, 20, 30]";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[1, 2, 3]", "engine={e}");
    }
}

#[test]
fn where_all_false_picks_ys() {
    let src = "f>L n;where [false, false, false] [1, 2, 3] [10, 20, 30]";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[10, 20, 30]", "engine={e}");
    }
}

#[test]
fn where_empty_lists() {
    let src = "f>L n;where [] [] []";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[]", "engine={e}");
    }
}

#[test]
fn where_preserves_text_element_type() {
    // Element type carries through: a list of text in, a list of text out.
    let src = r#"f>L t;where [true, false, true] ["a", "b", "c"] ["x", "y", "z"]"#;
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[a, y, c]", "engine={e}");
    }
}

#[test]
fn where_length_mismatch_errors() {
    // cond shorter than xs / ys — the runtime guard must catch this.
    let src = "f>L n;where [true, false] [1, 2, 3] [10, 20, 30]";
    for e in engines() {
        let stderr = run_err(e, src, "f");
        assert!(
            stderr.contains("ILO-R009"),
            "engine={e}: expected ILO-R009, got stderr={stderr}"
        );
        assert!(
            stderr.contains("where") && stderr.contains("length mismatch"),
            "engine={e}: expected message to mention where + length mismatch, got stderr={stderr}"
        );
    }
}

#[test]
fn where_xs_ys_length_mismatch_errors() {
    let src = "f>L n;where [true, false, true] [1, 2, 3] [10, 20]";
    for e in engines() {
        let stderr = run_err(e, src, "f");
        assert!(
            stderr.contains("ILO-R009"),
            "engine={e}: expected ILO-R009, got stderr={stderr}"
        );
        assert!(
            stderr.contains("length mismatch"),
            "engine={e}: expected length mismatch error, got stderr={stderr}"
        );
    }
}

// ---- Coverage: verify-time type errors (ILO-T013) -------------------------

#[test]
fn where_cond_not_bool_list_verify_error() {
    // First arg must be L b, not L n. Verifier rejects with ILO-T013.
    let src = "f>L n;where [1, 2, 3] [1, 2, 3] [10, 20, 30]";
    let stderr = run_err("--vm", src, "f");
    assert!(
        stderr.contains("ILO-T013"),
        "expected ILO-T013, got: {stderr}"
    );
    assert!(
        stderr.contains("where") && stderr.contains("L b"),
        "expected message to mention where + L b, got: {stderr}"
    );
}

#[test]
fn where_xs_ys_element_type_mismatch_verify_error() {
    // xs:L n vs ys:L t — the element types must unify.
    let src = r#"f>L _;where [true, false] [1, 2] ["x", "y"]"#;
    let stderr = run_err("--vm", src, "f");
    assert!(
        stderr.contains("ILO-T013"),
        "expected ILO-T013, got: {stderr}"
    );
    assert!(
        stderr.contains("xs and ys element types must match"),
        "expected element-type-mismatch hint, got: {stderr}"
    );
}

#[test]
fn where_cond_not_list_verify_error() {
    let src = "f>L n;where 5 [1, 2, 3] [10, 20, 30]";
    let stderr = run_err("--vm", src, "f");
    assert!(
        stderr.contains("ILO-T013"),
        "expected ILO-T013, got: {stderr}"
    );
}
