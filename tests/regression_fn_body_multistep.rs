// ILO-53: Single-expression function bodies: allow statement chains
//
// Verifies that multi-statement function bodies work across all engines in
// the three shapes the ticket requires:
//   1. Bind-chain:  `f x y { s = +x y; *s 2 }` — multiple let-bindings then tail
//   2. Early return: braceless guard `>=x 0 val` or explicit `ret val`
//   3. Result unwrap mid-body: `v = call!; use_v`
//
// This is a cross-engine regression gate; every shape must produce the same
// result on tree/VM and JIT.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, args: &[&str]) -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("ilo_53_{}_{}.ilo", std::process::id(), n));
    std::fs::write(&path, src).unwrap();
    let mut cmd = ilo();
    cmd.arg(path.to_str().unwrap()).arg(engine);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed:\nsrc={src}\nstderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// ── 1. Bind-chain (inline semicolons) ──────────────────────────────────────

const BIND_CHAIN_INLINE: &str = "add-and-double x:n y:n>n;s=+x y;*s 2\n";

#[test]
fn bind_chain_inline_vm() {
    assert_eq!(
        run("--vm", BIND_CHAIN_INLINE, &["add-and-double", "3", "4"]),
        "14"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn bind_chain_inline_jit() {
    assert_eq!(
        run("--jit", BIND_CHAIN_INLINE, &["add-and-double", "3", "4"]),
        "14"
    );
}

// ── 2. Bind-chain (brace-block form) ───────────────────────────────────────

const BIND_CHAIN_BRACE: &str = "add-and-double x:n y:n>n { s = +x y; *s 2 }\n";

#[test]
fn bind_chain_brace_vm() {
    assert_eq!(
        run("--vm", BIND_CHAIN_BRACE, &["add-and-double", "3", "4"]),
        "14"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn bind_chain_brace_jit() {
    assert_eq!(
        run("--jit", BIND_CHAIN_BRACE, &["add-and-double", "3", "4"]),
        "14"
    );
}

// ── 3. Early return via braceless guard ────────────────────────────────────
// `>=x 0 *x 10` — if x >= 0, return x*10 immediately; otherwise negate first.

const EARLY_RETURN_GUARD: &str = "abs-scale x:n>n;>=x 0 *x 10;neg=*x -1;*neg 10\n";

#[test]
fn early_return_guard_positive_vm() {
    assert_eq!(run("--vm", EARLY_RETURN_GUARD, &["abs-scale", "3"]), "30");
}

#[test]
fn early_return_guard_negative_vm() {
    assert_eq!(
        run("--vm", EARLY_RETURN_GUARD, &["abs-scale", "--", "-5"]),
        "50"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn early_return_guard_positive_jit() {
    assert_eq!(run("--jit", EARLY_RETURN_GUARD, &["abs-scale", "3"]), "30");
}

#[test]
#[cfg(feature = "cranelift")]
fn early_return_guard_negative_jit() {
    assert_eq!(
        run("--jit", EARLY_RETURN_GUARD, &["abs-scale", "--", "-5"]),
        "50"
    );
}

// ── 4. Early return via `ret` ───────────────────────────────────────────────
// `<=x 0{ret 0}` — if x <= 0, ret 0 early; otherwise return x itself.

const EARLY_RETURN_RET: &str = "clamp-pos x:n>n;<=x 0{ret 0};+x 0\n";

#[test]
fn early_return_ret_negative_vm() {
    assert_eq!(
        run("--vm", EARLY_RETURN_RET, &["clamp-pos", "--", "-3"]),
        "0"
    );
}

#[test]
fn early_return_ret_positive_vm() {
    assert_eq!(run("--vm", EARLY_RETURN_RET, &["clamp-pos", "7"]), "7");
}

#[test]
#[cfg(feature = "cranelift")]
fn early_return_ret_negative_jit() {
    assert_eq!(
        run("--jit", EARLY_RETURN_RET, &["clamp-pos", "--", "-3"]),
        "0"
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn early_return_ret_positive_jit() {
    assert_eq!(run("--jit", EARLY_RETURN_RET, &["clamp-pos", "7"]), "7");
}

// ── 5. Result unwrap mid-body (`!`) ────────────────────────────────────────
// `a=num! "10";b=num! "32";~+a b` — unwrap two Results, then wrap sum as Ok.

const RESULT_UNWRAP_BODY: &str = "parse-and-add>R n t;a=num! \"10\";b=num! \"32\";~+a b\n";

#[test]
fn result_unwrap_mid_body_vm() {
    assert_eq!(run("--vm", RESULT_UNWRAP_BODY, &["parse-and-add"]), "42");
}

#[test]
#[cfg(feature = "cranelift")]
fn result_unwrap_mid_body_jit() {
    assert_eq!(run("--jit", RESULT_UNWRAP_BODY, &["parse-and-add"]), "42");
}

// ── 6. Three-step bind-chain: ensures arbitrarily many lets work ───────────

const THREE_STEP: &str = "sum-of-sq a:n b:n>n;as=*a a;bs=*b b;+as bs\n";

#[test]
fn three_step_chain_vm() {
    assert_eq!(run("--vm", THREE_STEP, &["sum-of-sq", "3", "4"]), "25");
}

#[test]
#[cfg(feature = "cranelift")]
fn three_step_chain_jit() {
    assert_eq!(run("--jit", THREE_STEP, &["sum-of-sq", "3", "4"]), "25");
}

// ── 7. Existing single-expression bodies still work ────────────────────────

const SINGLE_EXPR: &str = "double x:n>n;*x 2\n";

#[test]
fn single_expr_body_unbroken_vm() {
    assert_eq!(run("--vm", SINGLE_EXPR, &["double", "5"]), "10");
}

#[test]
#[cfg(feature = "cranelift")]
fn single_expr_body_unbroken_jit() {
    assert_eq!(run("--jit", SINGLE_EXPR, &["double", "5"]), "10");
}
