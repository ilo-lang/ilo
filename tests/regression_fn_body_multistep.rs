// ILO-53 / ILO-406: Multi-statement function body regression gate.
//
// Verifies that multi-statement function bodies work across all engines in
// the three shapes the ticket requires:
//   1. Bind-chain:  `f x y { s = +x y; *s 2 }` — multiple let-bindings then tail
//   2. Early return: braceless guard `>=x 0 val` or explicit `ret val`
//   3. Result unwrap mid-body: `v = call!; use_v`
//
// ILO-406 extends ILO-53 coverage: AOT path for the Result-unwrap shape
// (`v=call!` mid-body) — `ilo compile` + run binary — in addition to the
// existing VM/JIT paths.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

static SEQ: AtomicU64 = AtomicU64::new(0);

fn tmp_src(tag: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("ilo_53_{tag}_{pid}_{n}.ilo"))
}

fn run(engine: &str, src: &str, args: &[&str]) -> String {
    let path = tmp_src("vm");
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

// ── 5. Result unwrap mid-body (`!`) — VM/JIT paths ─────────────────────────
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

// ── 5b. Result unwrap mid-body — AOT path (ILO-406) ────────────────────────
//
// `ilo compile` the same source, run the native binary, verify output matches
// the VM/JIT result. This is the regression specifically requested by ILO-406.

#[test]
#[cfg(feature = "cranelift")]
fn result_unwrap_mid_body_aot() {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let src_path = std::env::temp_dir().join(format!("ilo_406_unwrap_{pid}_{n}.ilo"));
    let bin_path = std::env::temp_dir().join(format!("ilo_406_unwrap_{pid}_{n}.bin"));

    std::fs::write(&src_path, RESULT_UNWRAP_BODY).unwrap();

    // Compile to native binary.
    let compile = ilo()
        .args(["compile"])
        .arg(&src_path)
        .arg("-o")
        .arg(&bin_path)
        .arg("parse-and-add")
        .output()
        .expect("failed to invoke ilo compile");
    assert!(
        compile.status.success(),
        "ilo compile failed:\nsrc={RESULT_UNWRAP_BODY}\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr),
    );

    // Run the native binary.
    let run_out = Command::new(&bin_path)
        .output()
        .expect("failed to run AOT binary");
    assert!(
        run_out.status.success(),
        "AOT binary exited non-zero:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&run_out.stdout),
        String::from_utf8_lossy(&run_out.stderr),
    );

    let stdout = String::from_utf8_lossy(&run_out.stdout).trim().to_string();
    assert_eq!(stdout, "42", "AOT: expected '42', got {stdout:?}");

    let _ = std::fs::remove_file(&src_path);
    let _ = std::fs::remove_file(&bin_path);
}

// ── 6. Three-step bind-chain ────────────────────────────────────────────────

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
