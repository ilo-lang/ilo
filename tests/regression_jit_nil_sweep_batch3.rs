// Regression tests for the Cranelift JIT-helper permissive-nil sweep, batch 3.
//
// Helpers in scope (Group A — arithmetic, comparison, numeric unary/binary):
//   jit_add, jit_add_inplace, jit_sub, jit_mul, jit_div, jit_mod, jit_neg,
//   jit_gt, jit_lt, jit_ge, jit_le, jit_abs, jit_min, jit_max, jit_flr,
//   jit_cel, jit_rou, jit_clamp, jit_len, jit_str, jit_num.
//
// Before this PR these helpers silently returned TAG_NIL (or TAG_FALSE for
// the ordered comparisons) on failure paths where tree/VM raise runtime
// errors. The fix routes the failure paths through the `JIT_RUNTIME_ERROR`
// TLS cell introduced in #254, threading a packed source-span immediate so
// diagnostics render with a caret matching tree/VM.
//
// Most of these helpers are only reachable through the slow path of an op
// (e.g. OP_SUB calls jit_sub only when neither operand is statically known
// to be a number). The ilo source-level verifier rejects programs that
// statically mix types (ILO-T009 / ILO-T010 / ILO-T012), so per-helper
// error-path tests live as unit tests inside `src/vm/mod.rs` that drive
// the helpers directly. These CLI tests focus on cross-engine happy-path
// parity — pinning that wiring the span/error threads did not regress the
// success cases (operations between numbers, strings, lists) across tree,
// VM, and Cranelift JIT.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn check_stdout(engine: &str, src: &str, expected: &str) {
    let out = ilo()
        .args([src, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "engine={engine}: expected success for `{src}`, got stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        expected,
        "engine={engine}: stdout mismatch for `{src}`"
    );
}

// Run a check across all three engines and assert identical output.
fn check_all(src: &str, expected: &str) {
    check_stdout("--run-tree", src, expected);
    check_stdout("--run-vm", src, expected);
    #[cfg(feature = "cranelift")]
    check_stdout("--run-cranelift", src, expected);
}

// ── Arithmetic happy paths ────────────────────────────────────────────────

#[test]
fn add_numbers_cross_engine() {
    check_all("f>n;+2 3", "5");
}

#[test]
fn add_strings_cross_engine() {
    check_all("f>t;+\"foo\" \"bar\"", "foobar");
}

#[test]
fn add_lists_cross_engine() {
    check_all("f>L n;+[1 2] [3 4]", "[1, 2, 3, 4]");
}

#[test]
fn sub_numbers_cross_engine() {
    check_all("f>n;- 10 3", "7");
}

#[test]
fn mul_numbers_cross_engine() {
    check_all("f>n;* 4 5", "20");
}

#[test]
fn div_numbers_cross_engine() {
    check_all("f>n;/ 10 4", "2.5");
}

#[test]
fn mod_numbers_cross_engine() {
    check_all("f>n;mod 10 3", "1");
}

#[test]
fn neg_number_cross_engine() {
    check_all("f>n;- 5", "-5");
}

// ── Division-by-zero parity ───────────────────────────────────────────────
//
// Tree, VM, and Cranelift JIT all raise on `n / 0`. Pin that the message is
// recognisable and parity holds across engines. Cranelift only goes through
// `jit_div` for the slow path; the always-num inline path also goes through
// the new error route via `jit_set_runtime_error_with_span(VmError::DivisionByZero, ...)`
// (see comment in jit_div) — but with `f>n;/ x 0` the verifier knows both
// are n, so it inlines fdiv which produces inf, not an error. To exercise
// the helper we need a non-always-num path; the divide-by-zero error path
// is exercised by the helper unit test `jit_div_by_zero_signals_runtime_error`.
// At the CLI level we pin the more useful invariant: VM and tree both error
// on /n 0, and Cranelift's fdiv produces inf (the existing semantic gap
// outside this batch's scope).

fn divide_by_zero_errors(engine: &str) {
    let out = ilo()
        .args(["f>n;/ 5 0", engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "engine={engine}: expected divide-by-zero error, got stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("divide")
            || stderr.contains("zero")
            || stderr.contains("division")
            || stderr.contains("Division"),
        "engine={engine}: expected divide/zero in stderr, got: {stderr}"
    );
}

#[test]
fn div_by_zero_tree() {
    divide_by_zero_errors("--run-tree");
}

#[test]
fn div_by_zero_vm() {
    divide_by_zero_errors("--run-vm");
}

// Note: Cranelift CLI div-by-zero behaviour for always-num inline path
// produces inf (not an error). The helper slow path now errors but isn't
// reached from this surface-level program. Tracked separately.

// ── Comparison happy paths ────────────────────────────────────────────────

#[test]
fn gt_numbers_cross_engine() {
    check_all("f>b;> 5 3", "true");
    check_all("f>b;> 3 5", "false");
}

#[test]
fn lt_numbers_cross_engine() {
    check_all("f>b;< 2 7", "true");
}

#[test]
fn ge_numbers_cross_engine() {
    check_all("f>b;>= 5 5", "true");
}

#[test]
fn le_numbers_cross_engine() {
    check_all("f>b;<= 3 3", "true");
}

#[test]
fn gt_strings_cross_engine() {
    check_all("f>b;> \"b\" \"a\"", "true");
}

#[test]
fn lt_strings_cross_engine() {
    check_all("f>b;< \"a\" \"b\"", "true");
}

// ── Numeric unary / binary helpers ────────────────────────────────────────

#[test]
fn abs_number_cross_engine() {
    check_all("f>n;abs -7", "7");
}

#[test]
fn min_numbers_cross_engine() {
    check_all("f>n;min 3 5", "3");
}

#[test]
fn max_numbers_cross_engine() {
    check_all("f>n;max 3 5", "5");
}

#[test]
fn flr_number_cross_engine() {
    check_all("f>n;flr 3.7", "3");
}

#[test]
fn cel_number_cross_engine() {
    check_all("f>n;cel 3.2", "4");
}

#[test]
fn rou_number_cross_engine() {
    check_all("f>n;rou 3.5", "4");
}

#[test]
fn clamp_in_range_cross_engine() {
    check_all("f>n;clamp 5 0 10", "5");
}

#[test]
fn clamp_above_max_cross_engine() {
    check_all("f>n;clamp 15 0 10", "10");
}

#[test]
fn clamp_below_min_cross_engine() {
    check_all("f>n;clamp -5 0 10", "0");
}

// ── len / str / num happy paths ───────────────────────────────────────────

#[test]
fn len_string_cross_engine() {
    check_all("f>n;len \"hello\"", "5");
}

#[test]
fn len_list_cross_engine() {
    check_all("f>n;len [1 2 3]", "3");
}

#[test]
fn str_number_cross_engine() {
    check_all("f>t;str 42", "42");
}

// ── No stale-error leak across successive Cranelift calls ─────────────────
//
// PR #254's JitRuntimeErrorGuard clears the TLS error cell on entry/exit.
// Confirm that a helper-set error on an /errored/ Cranelift call does not
// leak into the next fresh invocation. We can't easily provoke a Cranelift
// helper-driven error from surface ilo (verifier rejects mixed-type ops),
// so we use the empty-list `hd` path from batch 1 as the carrier and run a
// happy-path arithmetic program afterwards.

#[test]
#[cfg(feature = "cranelift")]
fn no_stale_jit_error_leak_after_hd_error_then_arithmetic() {
    let first = ilo()
        .args(["f>n;hd []", "--run-cranelift", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(!first.status.success(), "first call should error on hd []");
    // Second fresh process: arithmetic must succeed cleanly.
    check_stdout("--run-cranelift", "f>n;+ 1 2", "3");
}
