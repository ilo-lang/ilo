// Regression tests for the agent-natural surface (compat/agent-natural).
//
// Spec: SPEC-AGENT-NATURAL.md. Each new surface form is a parse-time desugar
// onto an existing AST node, so the verifier and every backend see no new
// shapes. These tests pin that the new forms run identically on the VM and
// Cranelift JIT backends — if any backend ever sees something it shouldn't,
// the abstraction has leaked and one of the engines will diverge.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str, arg: &str) -> String {
    let out = ilo()
        .args([src, engine, entry, arg])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// ── match arm block bodies ──────────────────────────────────────────────────
//
// Multi-statement match arm bodies via `pat:{stmt;stmt;expr}` already live in
// `parse_arm_body`. These tests pin the behaviour cross-engine so a future
// refactor can't silently drop them (spec §2.3 calls match-arm blocks out as
// a v0 item — confirming it works on both backends is the contract).

const MATCH_BLOCK_OK_ARM: &str = "go n:n>n;r=?n{0:^\"zero\";_:~n};?r{~v:{d=*v 2;+d 1};^_:0}\n";

#[test]
fn match_arm_block_ok_vm() {
    assert_eq!(run("--vm", MATCH_BLOCK_OK_ARM, "go", "10"), "21");
}

#[test]
#[cfg(feature = "cranelift")]
fn match_arm_block_ok_jit() {
    assert_eq!(run("--jit", MATCH_BLOCK_OK_ARM, "go", "10"), "21");
}

const MATCH_BLOCK_ERR_ARM: &str =
    "go n:n>t;r=?n{0:^\"oops\";_:~\"k\"};?r{~v:str v;^er:{tag=\"err: \";+tag er}}\n";

#[test]
fn match_arm_block_err_vm() {
    assert_eq!(run("--vm", MATCH_BLOCK_ERR_ARM, "go", "0"), "err: oops");
}

#[test]
#[cfg(feature = "cranelift")]
fn match_arm_block_err_jit() {
    assert_eq!(run("--jit", MATCH_BLOCK_ERR_ARM, "go", "0"), "err: oops");
}
