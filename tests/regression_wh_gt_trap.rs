// Regression: `wh >cond{...}` mid-body must parse as a while loop, not be
// mis-read as a fresh function declaration named `wh` returning some type.
//
// Before the fix, `is_fn_decl_start` accepted any `Ident Greater ...` as a
// zero-param fn header, so `parse_body_with`'s top-level boundary heuristic
// terminated the outer body at `wh` and tried to parse `wh >v 0{...}` as
// `wh` returning `v`. Symptoms: ILO-T008 "return type mismatch" plus a
// cascade of "undefined variable" errors with note `in function 'wh'`.
//
// Reported by gis-analyst and routing-tsp persona reruns against v0.11.1.
//
// Discriminator: reserved statement-keyword identifiers (`wh`/`ret`/`brk`/`cnt`)
// are intercepted by `parse_stmt` as control-flow forms and can never start a
// fn declaration. `is_fn_decl_start` now short-circuits to false for them.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str, arg: &str) -> (bool, String, String) {
    let mut cmd = ilo();
    cmd.args([src, engine, entry]);
    if !arg.is_empty() {
        cmd.arg(arg);
    }
    let out = cmd.output().expect("failed to run ilo");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
    )
}

// Headline gis-analyst repro: `wh >v 0` after a let binding.
const WH_GT_AFTER_LET: &str = "foo s:n>n;v=+s 0;wh >v 0{v=- v 1};+v 0";

fn check_wh_gt_after_let(engine: &str) {
    let (ok, stdout, stderr) = run(engine, WH_GT_AFTER_LET, "foo", "5");
    assert!(ok, "engine={engine}: stderr={stderr}");
    assert_eq!(stdout, "0", "engine={engine}");
}

#[test]
fn wh_gt_after_let_tree() {
    check_wh_gt_after_let("--run-tree");
}

#[test]
fn wh_gt_after_let_vm() {
    check_wh_gt_after_let("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn wh_gt_after_let_cranelift() {
    check_wh_gt_after_let("--run-cranelift");
}

// `wh >cond{...}` followed by a sibling function — must not slurp the next
// fn's header. The body-boundary heuristic should still find the real
// boundary at the `;` between the loop's closing `}` and the next fn header.
const WH_GT_THEN_SIBLING: &str = "foo s:n>n;v=+s 0;wh >v 0{v=- v 1};+v 0;main>n;foo 3";

fn check_wh_gt_then_sibling(engine: &str) {
    let (ok, stdout, stderr) = run(engine, WH_GT_THEN_SIBLING, "main", "");
    assert!(ok, "engine={engine}: stderr={stderr}");
    assert_eq!(stdout, "0", "engine={engine}");
}

#[test]
fn wh_gt_then_sibling_tree() {
    check_wh_gt_then_sibling("--run-tree");
}

#[test]
fn wh_gt_then_sibling_vm() {
    check_wh_gt_then_sibling("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn wh_gt_then_sibling_cranelift() {
    check_wh_gt_then_sibling("--run-cranelift");
}

// `wh >=v 0` (GreaterEq prefix) — same family, must not be misread.
const WH_GE: &str = "foo s:n>n;v=+s 0;wh >=v 1{v=- v 1};+v 0";

fn check_wh_ge(engine: &str) {
    let (ok, stdout, stderr) = run(engine, WH_GE, "foo", "5");
    assert!(ok, "engine={engine}: stderr={stderr}");
    assert_eq!(stdout, "0", "engine={engine}");
}

#[test]
fn wh_ge_tree() {
    check_wh_ge("--run-tree");
}

#[test]
fn wh_ge_vm() {
    check_wh_ge("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn wh_ge_cranelift() {
    check_wh_ge("--run-cranelift");
}

// Sanity: a legitimate zero-param fn decl still parses fine — the reserved
// shortlist (`wh`/`ret`/`brk`/`cnt`) is the only set that short-circuits.
// Multi-decl program with a zero-param fn first; the body-boundary heuristic
// must still find this boundary.
const ZERO_PARAM_FN_OK: &str = "answer>n;42;dbl x:n>n;*x 2";

fn check_zero_param_fn_ok(engine: &str) {
    let (ok, stdout, stderr) = run(engine, ZERO_PARAM_FN_OK, "answer", "");
    assert!(ok, "engine={engine}: stderr={stderr}");
    assert_eq!(stdout, "42", "engine={engine}");
}

#[test]
fn zero_param_fn_ok_tree() {
    check_zero_param_fn_ok("--run-tree");
}

#[test]
fn zero_param_fn_ok_vm() {
    check_zero_param_fn_ok("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn zero_param_fn_ok_cranelift() {
    check_zero_param_fn_ok("--run-cranelift");
}
