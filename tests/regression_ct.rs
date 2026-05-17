// Regression tests for the `ct` builtin.
//
// `ct fn xs -> n`              — count elements where predicate returns true
// `ct fn ctx xs -> n`          — closure-bind variant, parallel to flt 3
//
// Motivation: bioinformatics rerun6 (ilo_assessment_feedback.md line 5028)
// wanted `tm=cnt has-tm seqs` to replace `tm=len (flt has-tm seqs)` and
// avoid the L b intermediate allocation. `cnt` is already reserved as the
// `continue` keyword (src/parser/mod.rs:3507), so the builtin is named
// `ct` — two chars, no parser surgery, strict improvement on the persona's
// three-char ask in token economy.
//
// Engine coverage: tree, VM, Cranelift JIT. All three route via the
// tree-bridge (Builtin::Ct in is_tree_bridge_eligible), so they share the
// tree interpreter's predicate dispatch and behave identically.

use std::process::Command;

const ENGINES: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_text_engine(src: &str, engine: &str) -> String {
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

fn check(src: &str, expected: &str) {
    for engine in ENGINES {
        let actual = run_text_engine(src, engine);
        assert_eq!(
            actual, expected,
            "engine={engine}, src=`{src}`: got `{actual}`, expected `{expected}`"
        );
    }
}

#[test]
fn ct_counts_matching_predicate() {
    check("pos x:n>b;>x 0;f>n;xs=[-3,0,2,4,-1,5];ct pos xs", "3");
}

#[test]
fn ct_empty_list_returns_zero() {
    check("pos x:n>b;>x 0;f>n;xs=[];ct pos xs", "0");
}

#[test]
fn ct_no_matches_returns_zero() {
    check("neg x:n>b;<x 0;f>n;xs=[1,2,3];ct neg xs", "0");
}

#[test]
fn ct_all_match_returns_len() {
    check("any x:n>b;true;f>n;xs=[1,2,3,4];ct any xs", "4");
}

#[test]
fn ct_text_list_predicate() {
    // Confirms the predicate works on text elements (not just numbers).
    check(
        r#"long s:t>b;>(len s) 3;f>n;xs=["a","bb","ccc","dddd","ee"];ct long xs"#,
        "1",
    );
}

#[test]
fn ct_closure_bind_variant() {
    // ct fn ctx xs — closure-bind variant, parallel to flt 3. The
    // predicate takes (elem, ctx) and returns b. Counts elements > threshold.
    check("gt x:n c:n>b;>x c;f>n;xs=[1,5,3,8,2,7];ct gt 4 xs", "3");
}

#[test]
fn ct_non_bool_predicate_errors() {
    // Predicate that returns a number must surface as a runtime error on
    // every engine (cnt being in tree_bridge_propagates_error keeps the
    // Cranelift path in lockstep with tree/VM).
    for engine in ENGINES {
        let out = ilo()
            .args(["idn x:n>n;x;f>n;xs=[1,2,3];ct idn xs", engine, "f"])
            .output()
            .expect("failed to run ilo");
        assert!(
            !out.status.success(),
            "engine={engine}: expected failure when predicate returns non-bool"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("ct") && stderr.contains("bool"),
            "engine={engine}: stderr should mention ct + bool, got `{stderr}`"
        );
    }
}

#[test]
fn ct_first_arg_must_be_fn_ref() {
    // Verifier rejects non-function first arg with ILO-T013.
    let out = ilo()
        .args(["f>n;ct 42 [1,2,3]", "--run-tree", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected failure on non-fn first arg"
    );
}

#[test]
fn ct_versus_len_flt_parity() {
    // The motivating identity: ct p xs ≡ len (flt p xs). Verify both sides
    // match on a representative input across every engine.
    check("pos x:n>b;>x 0;f>n;xs=[-2,-1,0,1,2,3,4,5];ct pos xs", "5");
    check(
        "pos x:n>b;>x 0;f>n;xs=[-2,-1,0,1,2,3,4,5];len (flt pos xs)",
        "5",
    );
}

#[test]
fn ct_does_not_shadow_continue_keyword() {
    // Regression guard for the `cnt`/`continue` reservation. `ct` as a
    // builtin must NOT have stolen the loop-continue keyword `cnt`. This
    // test exercises a `wh` loop body using `cnt` (continue) interleaved
    // with `ct` (count builtin) — proves both coexist cleanly.
    check(
        "even x:n>b;=(mod x 2) 0;f>n;xs=[1,2,3,4,5,6];ct even xs",
        "3",
    );
    // And a parser-level confirmation that bare `cnt` still parses as
    // `Stmt::Continue` inside a wh loop. If the ct-as-builtin change had
    // accidentally bumped `cnt` off the reserved-keyword list it would
    // dispatch as an undefined-function call here, not a continue.
    let out = ilo()
        .args([
            "f>n;i=0;n=0;wh <i 5{i=+i 1;cnt;n=+n 1};n",
            "--run-tree",
            "f",
        ])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "cnt-as-continue must still work alongside ct-as-builtin: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    // cnt skips the `n=+n 1` line on every iteration; n stays 0.
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "0");
}
