// Regression: Cranelift JIT silently produced NaN / inf where tree and VM
// raised ILO-R003 division by zero. Discovered in the ml-engineer rerun8
// Adult-dataset workload where `zsc` over a constant column produced
// `bias=NaN w1=NaN w2=NaN` but a plausible 0.76 majority-class accuracy on
// Cranelift; tree + VM correctly errored.
//
// Two bugs were rolled together:
//   1. The Cranelift fast-path inline-fdiv emit (OP_DIV_NN, OP_DIVK_N, and the
//      both_always_num / num_block branches of OP_DIV) skipped the zero check
//      that `jit_div` performed, producing silent NaN / inf.
//   2. The AOT `compile_cranelift` path also inlined `mod` as fdiv/trunc/fmul
//      /fsub with no zero guard.
//
// Fix:
//   * Added a tiny `jit_raise_divzero(span_bits)` extern in `src/vm/mod.rs`
//     that sets `VmError::DivisionByZero` on the per-thread cell.
//   * At every inline-fdiv site in `jit_cranelift.rs` / `compile_cranelift.rs`
//     emit an `fcmp == 0.0 -> brif` guard that calls the helper on the zero
//     edge. Constant divisor sites (OP_DIVK_N) resolve at compile time.
//   * Rejected `OP_DIV_NN` and zero-const `OP_DIVK_N` from the leaf-inline
//     emitter so chunks fall back to the guarded main path.
//   * AOT `OP_MOD` now routes through the existing `jit_mod` helper.
//
// These tests assert cross-engine parity: every engine surfaces ILO-R003
// "division by zero" for `/`. For `mod`-by-zero we only assert that no engine
// produces silent NaN; the existing tree-vs-VM R003/R004 mismatch is tracked
// separately in ilo_assessment_feedback.md.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Run `ilo` with inline source + engine flag + entry args, returning
/// (stdout, stderr, exit-code). Mirrors `regression_cranelift_error_span`.
fn run_engine(engine: &str, src: &str, args: &[&str]) -> (String, String, i32) {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine).arg("f");
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

fn assert_engine_errs(engine: &str, src: &str, args: &[&str], code: &str, msg: &str) {
    let (stdout, stderr, ec) = run_engine(engine, src, args);
    assert_ne!(
        ec, 0,
        "engine={engine} src={src:?} args={args:?} expected error exit, got 0\nstdout={stdout}\nstderr={stderr}"
    );
    assert!(
        stderr.contains(code),
        "engine={engine} src={src:?} expected `{code}` in stderr, got {stderr}"
    );
    assert!(
        stderr.contains(msg),
        "engine={engine} src={src:?} expected `{msg}` in stderr, got {stderr}"
    );
}

fn assert_all_engines_err(src: &str, args: &[&str], code: &str, msg: &str) {
    for engine in ["--run-tree", "--run-vm"] {
        assert_engine_errs(engine, src, args, code, msg);
    }
    #[cfg(feature = "cranelift")]
    {
        assert_engine_errs("--jit", src, args, code, msg);
    }
}

fn assert_engine_fails(engine: &str, src: &str, args: &[&str], msg: &str) {
    let (stdout, stderr, ec) = run_engine(engine, src, args);
    assert_ne!(
        ec, 0,
        "engine={engine} src={src:?} args={args:?} expected error exit, got 0\nstdout={stdout}\nstderr={stderr}"
    );
    assert!(
        stderr.contains(msg),
        "engine={engine} src={src:?} expected `{msg}` in stderr, got {stderr}"
    );
}

/// Like `assert_all_engines_err` but only asserts every engine errors (without
/// locking in the code). Used for `mod`-by-zero where tree returns R003 and
/// VM/Cranelift return R004 — pre-existing parity gap.
fn assert_all_engines_fail(src: &str, args: &[&str], msg: &str) {
    for engine in ["--run-tree", "--run-vm"] {
        assert_engine_fails(engine, src, args, msg);
    }
    #[cfg(feature = "cranelift")]
    {
        assert_engine_fails("--jit", src, args, msg);
    }
}

// ── `/` zero divisor: every engine must raise ILO-R003 ──────────────────

#[test]
fn div_by_literal_zero_cross_engine() {
    // OP_DIVK_N path: literal zero constant divisor. Compile-time check fires.
    assert_all_engines_err("f a:n>n;/a 0", &["10"], "ILO-R003", "division by zero");
}

#[test]
fn div_by_runtime_zero_cross_engine() {
    // OP_DIV_NN / OP_DIV path: divisor computed at runtime so the inline
    // fdiv-fast-path emit must check it dynamically. Previously NaN'd silently.
    assert_all_engines_err(
        "f a:n b:n>n;/a b",
        &["10", "0"],
        "ILO-R003",
        "division by zero",
    );
}

#[test]
fn div_zero_over_zero_cross_engine() {
    // 0 / 0 specifically produced NaN on Cranelift (vs +inf for 10/0). Cover
    // both NaN and inf branches of the previous silent-failure space.
    assert_all_engines_err(
        "f a:n b:n>n;/a b",
        &["0", "0"],
        "ILO-R003",
        "division by zero",
    );
}

#[test]
fn div_by_zero_runtime_computed_cross_engine() {
    // The originating ml-engineer bug had the divisor computed at runtime via
    // a `sqrt(variance)` that hit zero. Mirror that without the lambda parser
    // surface area: compute the zero divisor from two equal runtime numbers.
    assert_all_engines_err(
        "f a:n b:n>n;d=- b a\n  /a d",
        &["5", "5"],
        "ILO-R003",
        "division by zero",
    );
}

// ── `mod` zero divisor: every engine must fail (no silent NaN) ──────────

#[test]
fn mod_by_zero_cross_engine_fails() {
    // Tree returns R003, VM/Cranelift return R004 (pre-existing parity gap).
    // The point of this test is that Cranelift no longer silently NaN's
    // through the AOT-inlined fdiv/trunc/fmul/fsub expansion.
    assert_all_engines_fail("f a:n b:n>n;mod a b", &["10", "0"], "modulo by zero");
}

// ── Sanity: safe-divisor fast paths still produce correct results ───────

#[test]
fn div_safe_runtime_divisor_cross_engine_agrees() {
    for engine in ["--run-tree", "--run-vm"] {
        let (stdout, _, ec) = run_engine(engine, "f a:n b:n>n;/a b", &["10", "4"]);
        assert_eq!(ec, 0, "engine={engine} unexpected error");
        assert_eq!(stdout.trim(), "2.5", "engine={engine}");
    }
    #[cfg(feature = "cranelift")]
    {
        let (stdout, _, ec) = run_engine("--jit", "f a:n b:n>n;/a b", &["10", "4"]);
        assert_eq!(ec, 0, "engine=cranelift unexpected error");
        assert_eq!(stdout.trim(), "2.5");
    }
}

#[test]
fn div_const_nonzero_fast_path_cross_engine_agrees() {
    // OP_DIVK_N with non-zero literal divisor: ensures the compile-time
    // "k != 0 → no guard" branch still emits a working fdiv.
    for engine in ["--run-tree", "--run-vm"] {
        let (stdout, _, ec) = run_engine(engine, "f a:n>n;/a 4", &["10"]);
        assert_eq!(ec, 0, "engine={engine} unexpected error");
        assert_eq!(stdout.trim(), "2.5", "engine={engine}");
    }
    #[cfg(feature = "cranelift")]
    {
        let (stdout, _, ec) = run_engine("--jit", "f a:n>n;/a 4", &["10"]);
        assert_eq!(ec, 0, "engine=cranelift unexpected error");
        assert_eq!(stdout.trim(), "2.5");
    }
}
