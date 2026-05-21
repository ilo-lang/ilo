// Cross-engine regression tests for `ewm` — exponential moving average.
//
// `ewm xs a > L n` returns the EWMA of `xs` with smoothing factor `a` in
// `[0, 1]`: `ewm[0] = xs[0]`, `ewm[i] = a*xs[i] + (1-a)*ewm[i-1]`.
//
// Tree-bridge eligible (see is_tree_bridge_eligible in src/vm/mod.rs), so the
// VM and Cranelift JIT inherit it through OP_CALL_BUILTIN_TREE. The tests
// fan across tree, register VM, and (when built with --features cranelift)
// the JIT to catch any future divergence.

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

#[test]
fn ewm_basic_half_alpha() {
    // ewm [1 2 3 4 5] 0.5 → [1, 1.5, 2.25, 3.125, 4.0625]
    let src = "f>L n;ewm [1, 2, 3, 4, 5] 0.5";
    for e in engines() {
        let got = run_ok(e, src, "f");
        assert_eq!(got, "[1, 1.5, 2.25, 3.125, 4.0625]", "engine={e}");
    }
}

#[test]
fn ewm_empty_list_is_empty() {
    let src = "f>L n;ewm [] 0.5";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[]", "engine={e}");
    }
}

#[test]
fn ewm_single_element_unchanged() {
    // First element always seeds verbatim, regardless of alpha.
    let src = "f>L n;ewm [42] 0.3";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[42]", "engine={e}");
    }
}

#[test]
fn ewm_alpha_zero_freezes_first_value() {
    // a=0 means "ignore new input"; output is a constant run of xs[0].
    let src = "f>L n;ewm [1, 2, 3, 4, 5] 0";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[1, 1, 1, 1, 1]", "engine={e}");
    }
}

#[test]
fn ewm_alpha_one_is_identity() {
    // a=1 means "fully react to new input"; output equals input.
    let src = "f>L n;ewm [1, 2, 3, 4, 5] 1";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[1, 2, 3, 4, 5]", "engine={e}");
    }
}

#[test]
fn ewm_alpha_above_one_errors() {
    let src = "f>L n;ewm [1, 2, 3] 1.5";
    for e in engines() {
        let stderr = run_err(e, src, "f");
        assert!(
            stderr.contains("ewm") && stderr.contains("[0, 1]"),
            "engine={e}: stderr={stderr}"
        );
        assert!(
            stderr.contains("ILO-R009"),
            "engine={e}: expected ILO-R009, got stderr={stderr}"
        );
    }
}

#[test]
fn ewm_alpha_below_zero_errors() {
    // The verifier accepts negative literals through `0 - x`; runtime guard
    // must reject any alpha outside [0, 1].
    let src = "f>L n;a=0 - 0.1;ewm [1, 2, 3] a";
    for e in engines() {
        let stderr = run_err(e, src, "f");
        assert!(
            stderr.contains("ewm") && stderr.contains("[0, 1]"),
            "engine={e}: stderr={stderr}"
        );
        assert!(
            stderr.contains("ILO-R009"),
            "engine={e}: expected ILO-R009, got stderr={stderr}"
        );
    }
}

#[test]
fn ewm_constant_input_is_constant() {
    // Any constant input is a fixed point of the EWMA recurrence.
    let src = "f>L n;ewm [7, 7, 7, 7] 0.3";
    for e in engines() {
        assert_eq!(run_ok(e, src, "f"), "[7, 7, 7, 7]", "engine={e}");
    }
}
