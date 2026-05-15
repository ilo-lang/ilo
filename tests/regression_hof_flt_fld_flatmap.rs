// Regression tests for native `flt` / `fld` / `flatmap` HOF dispatch on
// every engine (PR 3a of the VM/Cranelift HOF dispatch chain).
//
// Background: PR 1 (#274) landed FnRef NaN-tagging, PR 2 (#277) lifted
// `map fn xs` to a native bytecode loop using OP_CALL_DYN. PR 3a extends
// that pattern to the next three pure-bytecode HOFs:
//
//   - `flt fn xs`       filter, keep where fn returns true
//   - `fld fn xs init`  left fold with explicit initial accumulator
//   - `flatmap fn xs`   map then flatten one level (nested foreach over
//                       each call result inside the outer iteration)
//
// Cranelift gets all three for free via the existing `jit_call_dyn`
// helper (#277): the new compiler arms emit the same OP_CALL_DYN
// opcode, and the lowering is op-agnostic.
//
// The tests below pin the value-level behaviour across `--run-tree`,
// `--run-vm` and `--run-cranelift`. They cover the common shapes that
// were previously gated with `engine-skip: vm / cranelift`:
//   - user-function callbacks
//   - builtin callbacks (where the verifier promotes a pure builtin to F)
//   - empty list (early-exit before the first OP_CALL_DYN)
//   - single-element list (one trip through the loop body)
//   - composition with `map` (chained HOFs)
//
// Any divergence between engines should fail one of these tests rather
// than silently producing different output.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(name: &str, src: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "ilo_hof_flt_fld_flatmap_{name}_{}_{n}.ilo",
        std::process::id()
    ));
    std::fs::write(&path, src).expect("write src");
    path
}

fn run_ok(engine: &str, src: &str, entry: &str, args: &[&str]) -> String {
    let path = write_src(entry, src);
    let mut cmd = ilo();
    cmd.arg(&path).arg(engine).arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_all(src: &str, entry: &str, args: &[&str], expected: &str) {
    for engine in ["--run-tree", "--run-vm", "--run-cranelift"] {
        let actual = run_ok(engine, src, entry, args);
        assert_eq!(
            actual, expected,
            "engine {engine} produced {actual:?}, expected {expected:?} for src `{src}`"
        );
    }
}

// ── flt ─────────────────────────────────────────────────────────────────

const FLT_USER_POS: &str = "pos x:n>b;>x 0\nmain xs:L n>L n;flt pos xs";

#[test]
fn flt_user_fn_positives_tree_vm_cranelift() {
    run_all(FLT_USER_POS, "main", &["[-3,-1,0,2,4]"], "[2, 4]");
}

#[test]
fn flt_user_fn_empty_list() {
    // Empty list short-circuits at OP_FOREACHPREP — never reaches the
    // bool typecheck or OP_CALL_DYN.
    run_all(FLT_USER_POS, "main", &["[]"], "[]");
}

#[test]
fn flt_user_fn_all_pass() {
    run_all(FLT_USER_POS, "main", &["[1,2,3]"], "[1, 2, 3]");
}

#[test]
fn flt_user_fn_all_fail() {
    // Pins that the result list survives RC accounting when no
    // OP_LISTAPPEND fires in the body.
    run_all(FLT_USER_POS, "main", &["[-3,-1,0]"], "[]");
}

// Builtin-callback coverage is pinned by `map_builtin_abs_round_trip` in
// regression_hof_map.rs. The Cranelift `jit_call_dyn` helper is
// dispatch-uniform across HOFs, so once builtin-callback works for one
// HOF it works for all of them. The flt-specific bool-typecheck path
// makes it awkward to pick a numeric pure builtin that returns bool, so
// we lean on the map test for builtin coverage and keep flt focused on
// user-fn predicates.

// ── fld ─────────────────────────────────────────────────────────────────

const FLD_USER_ADD: &str = "add a:n b:n>n;+a b\nmain xs:L n>n;fld add xs 0";

#[test]
fn fld_user_fn_sum_tree_vm_cranelift() {
    run_all(FLD_USER_ADD, "main", &["[1,2,3,4,5]"], "15");
}

#[test]
fn fld_user_fn_empty_list_returns_init() {
    // Empty list: zero iterations, acc stays at init.
    run_all(FLD_USER_ADD, "main", &["[]"], "0");
}

#[test]
fn fld_user_fn_single_element() {
    // One iteration: pins acc=init, then acc=fn(init, item).
    run_all(FLD_USER_ADD, "main", &["[7]"], "7");
}

// Text concat fold: pins that the accumulator survives type changes
// (init is text, the fn returns text every iter).
const FLD_TEXT_CONCAT: &str = "join a:t b:t>t;+a b\nmain xs:L t>t;fld join xs \"\"";

#[test]
fn fld_text_concat() {
    run_all(FLD_TEXT_CONCAT, "main", &["[\"a\",\"b\",\"c\"]"], "abc");
}

// ── flatmap ─────────────────────────────────────────────────────────────

// Repeat each number n times: 2 -> [2, 2], 3 -> [3, 3, 3].
const FLATMAP_USER_REP: &str =
    "rep n:n>L n;xs=[];@i 0..n{xs=+=xs n};xs\nmain xs:L n>L n;flatmap rep xs";

#[test]
fn flatmap_user_fn_repeat_tree_vm_cranelift() {
    run_all(FLATMAP_USER_REP, "main", &["[1,2,3]"], "[1, 2, 2, 3, 3, 3]");
}

#[test]
fn flatmap_user_fn_empty_outer() {
    run_all(FLATMAP_USER_REP, "main", &["[]"], "[]");
}

#[test]
fn flatmap_user_fn_empty_inner_results() {
    // rep 0 returns [], so every outer element contributes nothing.
    // Pins that the inner FOREACHPREP correctly short-circuits on each
    // empty result without leaving the outer loop in a bad state.
    run_all(FLATMAP_USER_REP, "main", &["[0,0,0]"], "[]");
}

#[test]
fn flatmap_user_fn_mixed_inner_sizes() {
    // Mixes empty, single-element, and multi-element inner results to
    // exercise every branch of the inner loop in one go.
    run_all(
        FLATMAP_USER_REP,
        "main",
        &["[0,1,2,0,3]"],
        "[1, 2, 2, 3, 3, 3]",
    );
}

// ── Composition with map ────────────────────────────────────────────────

// Pins that flt's result is a valid List that feeds into map without
// corruption (RC accounting on the acc_reg survives OP_RET).
const FLT_THEN_MAP: &str = "pos x:n>b;>x 0\nsq x:n>n;*x x\nmain xs:L n>L n;map sq (flt pos xs)";

#[test]
fn flt_then_map_composition() {
    run_all(FLT_THEN_MAP, "main", &["[-2,-1,0,1,2,3]"], "[1, 4, 9]");
}

// Pins that fld's accumulator survives feeding it through map first.
const MAP_THEN_FLD: &str = "sq x:n>n;*x x\nadd a:n b:n>n;+a b\nmain xs:L n>n;fld add (map sq xs) 0";

#[test]
fn map_then_fld_composition() {
    run_all(MAP_THEN_FLD, "main", &["[1,2,3]"], "14");
}
