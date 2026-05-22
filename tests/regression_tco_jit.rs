//! Regression tests for Cranelift JIT tail-call elimination via `return_call`.
//!
//! PR3 of the TCO series migrates all JIT-compiled ilo functions to
//! `CallConv::Tail` and emits `return_call` at every `OP_TAILCALL` site.
//! These tests confirm that deep tail-recursive programs run to completion
//! on the JIT engine without stack overflow.

#![cfg(feature = "cranelift")]

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_jit(src: &str, entry: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.args(["--jit", src, entry]);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("ilo --jit failed to spawn");
    assert!(
        out.status.success(),
        "jit exited non-zero: {:?}",
        out.status
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Self-recursive tail call, 200 000 deep — would overflow stack without TCO.
#[test]
fn tco_jit_countdown_200k() {
    let src = "count-down n:n>n;=n 0 0;count-down -n 1\nmain>n;count-down 200000";
    assert_eq!(run_jit(src, "main", &[]), "0");
}

/// Self-recursive tail call, 1 000 000 deep.
#[test]
fn tco_jit_countdown_1m() {
    let src = "count-down n:n>n;=n 0 0;count-down -n 1\nmain>n;count-down 1000000";
    assert_eq!(run_jit(src, "main", &[]), "0");
}

/// Mutual tail recursion (ev ↔ od), 200 000 steps.
#[test]
fn tco_jit_mutual_200k() {
    let src = "ev n:n>n;=n 0 1;od -n 1\nod n:n>n;=n 0 0;ev -n 1\nev200k>n;ev 200000";
    assert_eq!(run_jit(src, "ev200k", &[]), "1");
}

/// Self-recursive accumulator (mysum), exercises a numeric arg pair.
#[test]
fn tco_jit_sum_accumulator_100k() {
    // sum from 1 to 100000 = 5000050000; tail-recursive accumulator pattern.
    let src = "mysum n:n acc:n>n;=n 0 acc;mysum -n 1 +acc n\nmain>n;mysum 100000 0";
    assert_eq!(run_jit(src, "main", &[]), "5000050000");
}
