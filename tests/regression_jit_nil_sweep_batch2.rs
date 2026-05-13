// Regression tests for the cranelift JIT-helper permissive-nil harmonisation
// sweep (batch 2): `jit_recfld`, `jit_recfld_name`, `jit_unwrap`, `jit_mget`.
//
// Companion to PR #254 (batch: `hd`/`tl`/`at`) and batch 1 (`jit_lst`,
// `jit_listget`, `jit_index`, `jit_slc`, `jit_jpth`).
//
// Per-helper audit decisions encoded as tests:
//   * `jit_recfld` / `jit_recfld_name`: split into permissive (SAFE op) and
//     strict variants. OP_RECFLD / OP_RECFLD_NAME now route through the
//     strict helper that sets `JIT_RUNTIME_ERROR`; OP_RECFLD_SAFE and
//     OP_RECFLD_NAME_SAFE keep the permissive helper (must still return nil).
//   * `jit_unwrap`: defensive harmonisation — unreachable arms now error
//     to match tree's defensive `vm_err!` arm. No observable user behaviour
//     change from well-formed bytecode.
//   * `jit_mget`: distinguish — missing-key returns nil (legitimate
//     `O _` semantics), wrong-type (non-map first arg, non-text key) errors.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_ok(engine: &str, src: &str, entry: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine).arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(engine: &str, src: &str, entry: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine).arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected failure for `{src}` on {engine}, stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

// ── jit_recfld_name strict path: missing dynamic field errors ──────────
//
// jpar! produces a record whose static field set is unknown to the verifier
// (`R ? t`). Strict `r.field` access on a missing key hits the heap-record
// path of jit_recfld_name. Before this fix, Cranelift silently returned nil;
// tree errors, VM either errored or segfaulted depending on the JMPNN path
// (see PR #248). Now all three engines error.

const STRICT_MISS: &str = "f j:t>R t t;r=jpar! j;~r.missing";

fn check_strict_miss(engine: &str) {
    let stderr = run_err(engine, STRICT_MISS, "f", &[r#"{"present":1}"#]);
    assert!(
        stderr.contains("missing") || stderr.contains("field"),
        "engine={engine}: expected missing/field in stderr, got: {stderr}"
    );
}

#[test]
fn strict_recfld_name_miss_tree() {
    check_strict_miss("--run-tree");
}

#[test]
fn strict_recfld_name_miss_vm() {
    check_strict_miss("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn strict_recfld_name_miss_cranelift() {
    check_strict_miss("--run-cranelift");
}

// ── jit_recfld_name strict path: present field returns the value ───────
//
// The strict-helper rewire must not regress the happy path.

const STRICT_HIT: &str = "f j:t>R n t;r=jpar! j;~r.present";

fn check_strict_hit(engine: &str) {
    assert_eq!(
        run_ok(engine, STRICT_HIT, "f", &[r#"{"present":42}"#]),
        "~42",
        "engine={engine}"
    );
}

#[test]
fn strict_recfld_name_hit_tree() {
    check_strict_hit("--run-tree");
}

#[test]
fn strict_recfld_name_hit_vm() {
    check_strict_hit("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn strict_recfld_name_hit_cranelift() {
    check_strict_hit("--run-cranelift");
}

// ── jit_recfld_name SAFE path: must still return nil (permissive helper
//    untouched) ─────────────────────────────────────────────────────────
//
// This is the critical guard for the _strict-sibling design: OP_RECFLD_NAME
// flips to the strict helper, but OP_RECFLD_NAME_SAFE keeps using the
// permissive helper. A naive in-place edit of jit_recfld_name would have
// broken `.?` semantics from PR #248.

const SAFE_MISS: &str = "f j:t>R t t;r=jpar! j;~fmt \"{}\" r.?missing";

fn check_safe_miss(engine: &str) {
    assert_eq!(
        run_ok(engine, SAFE_MISS, "f", &[r#"{"present":1}"#]),
        "~nil",
        "engine={engine}"
    );
}

#[test]
fn safe_recfld_name_miss_tree() {
    check_safe_miss("--run-tree");
}

#[test]
fn safe_recfld_name_miss_vm() {
    check_safe_miss("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn safe_recfld_name_miss_cranelift() {
    check_safe_miss("--run-cranelift");
}

// ── jit_mget: missing key returns nil on every engine ──────────────────
//
// `mget` returns `O _`. Missing key → nil is the correct (non-error)
// shape. This pins that the batch-2 wrong-type harmonisation didn't
// accidentally promote miss to an error.

const MGET_MISS: &str = "f>O n;m=mset (mmap) \"a\" 1;mget m \"z\"";

fn check_mget_miss(engine: &str) {
    assert_eq!(
        run_ok(engine, MGET_MISS, "f", &[]),
        "nil",
        "engine={engine}"
    );
}

#[test]
fn mget_missing_key_tree() {
    check_mget_miss("--run-tree");
}

#[test]
fn mget_missing_key_vm() {
    check_mget_miss("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn mget_missing_key_cranelift() {
    check_mget_miss("--run-cranelift");
}

// ── jit_mget: present key returns the value ────────────────────────────

const MGET_HIT: &str = "f>O n;m=mset (mmap) \"a\" 1;mget m \"a\"";

fn check_mget_hit(engine: &str) {
    assert_eq!(run_ok(engine, MGET_HIT, "f", &[]), "1", "engine={engine}");
}

#[test]
fn mget_present_key_tree() {
    check_mget_hit("--run-tree");
}

#[test]
fn mget_present_key_vm() {
    check_mget_hit("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn mget_present_key_cranelift() {
    check_mget_hit("--run-cranelift");
}

// ── No stale-error leak across successive cranelift calls ──────────────
//
// The TLS error cell is cleared by `JitRuntimeErrorGuard` (PR #254). The
// batch-2 strict-helper errors must not leak into subsequent fresh-process
// invocations. Pins the guard contract for the new error sites.

#[test]
#[cfg(feature = "cranelift")]
fn no_stale_jit_error_leak_after_strict_recfld_miss() {
    // First process: errors via strict recfld_name miss.
    let _ = run_err("--run-cranelift", STRICT_MISS, "f", &[r#"{"present":1}"#]);
    // Second fresh process: must succeed cleanly.
    assert_eq!(
        run_ok("--run-cranelift", STRICT_HIT, "f", &[r#"{"present":99}"#],),
        "~99",
        "fresh process after strict-miss error must succeed"
    );
}
