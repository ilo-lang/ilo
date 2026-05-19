// Regression tests for the default engine flip: bytecode register VM is the
// default execution path, Cranelift JIT is opt-in via `--jit`. Tree
// interpreter remains the canonical-semantics fallback for any program the VM
// rejects.
//
// Pre-flip: default tried Cranelift JIT first then fell back to the VM on
// every NotEligible bailout, paying compile-and-bail cost on programs that
// touch any opcode the JIT can't yet handle (OP_WINDOW_VIEW pre-#386,
// OP_LEN_HAS_K_COUNT, OP_MAKE_CLOSURE for Phase 2 captures pre-#385). After
// the flip, default goes straight to the VM; agents opt into the JIT
// explicitly for hot numeric loops.
//
// What we assert:
//   1. `ilo file.ilo` (default) and `ilo file.ilo --vm` produce
//      identical stdout for a VM-supported workload.
//   2. `ilo file.ilo --jit` runs the workload and produces correct output
//      (the JIT opt-in flag works).
//   3. Default invocation of a JIT-eligible workload completes WITHOUT a
//      JIT-fallback breadcrumb on stderr (proves we're using the VM
//      directly, not JIT-then-fallback).

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_args(args: &[&str]) -> (String, String, i32) {
    let out = ilo().args(args).output().expect("failed to run ilo");
    (
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.code().unwrap_or(-1),
    )
}

// A VM-supported numeric workload (Cranelift-eligible too — used to assert
// parity between default and `--vm`).
const NUMERIC_SRC: &str = "main x:n>n;y=*x 2;+y 1";

// A modern workload that exercises OP_WINDOW_VIEW (post-#386 Cranelift can
// JIT this, but historically bailed). Used to assert "no breadcrumb" on the
// default path — proves we are not even *attempting* the JIT.
const WINDOW_SRC: &str = "main>n;xs=[1,2,3,4,5];ws=window 3 xs;len ws";

// Modern workload exercising OP_LEN_HAS_K_COUNT (the fused len-of-filter
// peephole from #346). Tree/VM both run it; default must run it without a
// JIT breadcrumb.
const LEN_HAS_K_SRC: &str = "f x:n>b;>x 2\nmain>n;xs=[1,2,3,4,5];len (flt f xs)";

#[test]
fn default_matches_run_vm_numeric() {
    let (default_out, _, default_code) = run_args(&[NUMERIC_SRC, "main", "7"]);
    let (vm_out, _, vm_code) = run_args(&["--vm", NUMERIC_SRC, "main", "7"]);
    assert_eq!(default_code, 0, "default exit");
    assert_eq!(vm_code, 0, "--vm exit");
    assert_eq!(default_out, "15");
    assert_eq!(default_out, vm_out, "default vs --vm parity");
}

#[test]
fn jit_flag_runs_numeric() {
    let (out, _, code) = run_args(&["--jit", NUMERIC_SRC, "main", "7"]);
    assert_eq!(code, 0, "--jit exit");
    assert_eq!(out, "15");
}

#[test]
fn default_does_not_emit_jit_fallback_breadcrumb_on_window() {
    // OP_WINDOW_VIEW workload. If the default were still JIT-first, this
    // would either succeed via the post-#386 JIT path or fall back through
    // the NotEligible arm. Either way, on the new VM-direct default we must
    // observe no `[ilo:jit-fallback]` breadcrumb because the JIT is never
    // invoked at all.
    let (out, stderr, code) = run_args(&[WINDOW_SRC, "main"]);
    assert_eq!(code, 0, "exit, stderr={stderr}");
    assert_eq!(out, "3");
    assert!(
        !stderr.contains("[ilo:jit-fallback]"),
        "default path must not invoke JIT, stderr={stderr:?}",
    );
}

#[test]
fn default_does_not_emit_jit_fallback_breadcrumb_on_len_has() {
    let (out, stderr, code) = run_args(&[LEN_HAS_K_SRC, "main"]);
    assert_eq!(code, 0, "exit, stderr={stderr}");
    assert_eq!(out, "3");
    assert!(
        !stderr.contains("[ilo:jit-fallback]"),
        "default path must not invoke JIT, stderr={stderr:?}",
    );
}

#[test]
fn default_matches_run_vm_on_window() {
    let (default_out, _, default_code) = run_args(&[WINDOW_SRC, "main"]);
    let (vm_out, _, vm_code) = run_args(&["--vm", WINDOW_SRC, "main"]);
    assert_eq!(default_code, 0);
    assert_eq!(vm_code, 0);
    assert_eq!(default_out, vm_out, "default and --vm must agree");
}
