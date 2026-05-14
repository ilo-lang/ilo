// Regression tests for native `map` HOF dispatch on every engine
// (PR 2 of the VM/Cranelift HOF dispatch chain).
//
// Background: PR 1 (#274) landed FnRef NaN-tagging so a function
// reference could survive the NanVal round-trip on VM and Cranelift.
// PR 2 puts that plumbing to work: the compiler emits a native bytecode
// loop for `map fn xs` that calls back into the FnRef via OP_CALL_DYN,
// and Cranelift lowers OP_CALL_DYN to a `jit_call_dyn` helper which
// dispatches user-fns by re-entering the VM and builtins by routing
// through the tree-bridge. Every engine now runs `map` end-to-end.
//
// The tests below pin the value-level behaviour across `--run-tree`,
// `--run-vm` and `--run-cranelift`. They cover the common shapes that
// were previously gated with `engine-skip: vm / cranelift`:
//   - user-function callback (`map sq xs`)
//   - builtin callback (`map abs xs`)
//   - empty list (early-exit before the first OP_CALL_DYN)
//   - single-element list (one trip through the loop body)
//   - composition with the result of another `map` (chained HOFs)
//   - tail-call shape where `map` is the function's return value
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
    path.push(format!("ilo_hof_map_{name}_{}_{n}.ilo", std::process::id()));
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

// ── User-fn callback ────────────────────────────────────────────────────

const MAP_USER_SQ: &str = "sq x:n>n;*x x\nmain xs:L n>L n;map sq xs";

#[test]
fn map_user_fn_squares_tree_vm_cranelift() {
    run_all(MAP_USER_SQ, "main", &["[1,2,3,4,5]"], "[1, 4, 9, 16, 25]");
}

#[test]
fn map_user_fn_empty_list() {
    // Empty list short-circuits at OP_FOREACHPREP: the first JMP exits
    // before any OP_CALL_DYN fires. Pins the dispatcher's "no callback
    // for empty input" path on every engine.
    run_all(MAP_USER_SQ, "main", &["[]"], "[]");
}

#[test]
fn map_user_fn_single_element() {
    // One iteration only. Worth pinning separately because it exercises
    // OP_FOREACHPREP-stay-in-loop, then exactly one OP_CALL_DYN, then
    // FOREACHNEXT-fallthrough-to-exit. Past HOF stubs failed here
    // because the OP_LISTAPPEND in the body left acc_reg in a bad state.
    run_all(MAP_USER_SQ, "main", &["[7]"], "[49]");
}

// ── Builtin callback ────────────────────────────────────────────────────

// `abs` is a pure builtin promoted to F by the verifier (#165); the
// Cranelift path goes through `jit_call_dyn` → tree-bridge for builtins.
const MAP_BUILTIN_ABS: &str = "main xs:L n>L n;map abs xs";

#[test]
fn map_builtin_abs_round_trip() {
    run_all(MAP_BUILTIN_ABS, "main", &["[-3, 0, 4, -7]"], "[3, 0, 4, 7]");
}

// ── Chained map (composition) ───────────────────────────────────────────

// Two HOF calls in series: the result of the first feeds the second.
// This pins that `map`'s result is a valid List value (not a smuggled
// FnRef or stale heap pointer) when it flows back into another HOF.
const MAP_CHAINED: &str = "sq x:n>n;*x x\nadd1 x:n>n;+x 1\nmain xs:L n>L n;map add1 (map sq xs)";

#[test]
fn map_chained_user_fns() {
    run_all(MAP_CHAINED, "main", &["[1,2,3]"], "[2, 5, 10]");
}

// ── Tail-position map (result is the function's return) ─────────────────

// Most function bodies in ilo return the last expression. With `map` as
// the tail, the result register must survive OP_RET's RC accounting on
// every engine. Pins that we don't drop the result list early.
const MAP_TAIL: &str = "dbl x:n>n;*x 2\nmain xs:L n>L n;map dbl xs";

#[test]
fn map_tail_position_user_fn() {
    run_all(MAP_TAIL, "main", &["[5, 10, 15]"], "[10, 20, 30]");
}
