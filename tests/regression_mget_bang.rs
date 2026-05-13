// Regression tests for `!` auto-unwrap on Optional-returning builtins.
//
// Background: until this change, `mget!` (and any other O-returning builtin)
// was rejected by the verifier with ILO-T025 because `!` only accepted callees
// whose return type was `R _ _`. Every consumer of Optional ended up writing
// the same two-step `r=mget m k;v=r??default` bind to extract a value, even
// when the key was known to be present and nil-propagation was the desired
// behaviour on miss.
//
// `!` on an O-returning call is now defined as:
//   - if the result is `Some(v)` (i.e. non-nil at runtime), the call yields v
//   - if the result is nil, propagate nil as the enclosing function's return
//     (parallel to how `!` on `R _ _` propagates `Err`)
//
// The verifier requires the enclosing function's return type to accept nil
// (Optional or Unknown); otherwise it emits ILO-T026.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} unexpectedly succeeded for `{src}`: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

// ── present key: mget! returns the inner value ───────────────────────────
const PRESENT_SRC: &str = r#"f>O n;m=mset mmap "k" 5;v=mget! m "k";v"#;

fn check_present(engine: &str) {
    assert_eq!(run(engine, PRESENT_SRC, "f"), "5", "engine={engine}");
}

#[test]
fn mget_bang_present_tree() {
    check_present("--run-tree");
}

#[test]
fn mget_bang_present_vm() {
    check_present("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn mget_bang_present_cranelift() {
    check_present("--run-cranelift");
}

// ── missing key: mget! propagates nil out of the enclosing function ──────
const MISSING_SRC: &str = r#"f>O n;m=mmap;v=mget! m "missing";v"#;

fn check_missing(engine: &str) {
    assert_eq!(run(engine, MISSING_SRC, "f"), "nil", "engine={engine}");
}

#[test]
fn mget_bang_missing_tree() {
    check_missing("--run-tree");
}

#[test]
fn mget_bang_missing_vm() {
    check_missing("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn mget_bang_missing_cranelift() {
    check_missing("--run-cranelift");
}

// ── mget! propagation short-circuits subsequent statements ───────────────
// If mget! propagates nil from line 2, the `99` on line 3 must not execute.
const SHORTCIRCUIT_SRC: &str = r#"f>O n;m=mmap;v=mget! m "k";+v 99"#;

fn check_shortcircuit(engine: &str) {
    assert_eq!(run(engine, SHORTCIRCUIT_SRC, "f"), "nil", "engine={engine}");
}

#[test]
fn mget_bang_shortcircuit_tree() {
    check_shortcircuit("--run-tree");
}

#[test]
fn mget_bang_shortcircuit_vm() {
    check_shortcircuit("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn mget_bang_shortcircuit_cranelift() {
    check_shortcircuit("--run-cranelift");
}

// ── verifier rejects mget! in a non-Optional-returning function ──────────
// `f>n;...mget! m "k"` declares a numeric return; nil can't flow through it.
// ILO-T026 is the same diagnostic used for Result mismatch — the message
// distinguishes "not a Result" vs "not an Optional".
#[test]
fn mget_bang_in_non_optional_fn_rejected() {
    let stderr = run_err("--run-vm", r#"f>n;m=mmap;mget! m "x""#, "f");
    assert!(
        stderr.contains("ILO-T026") && stderr.contains("Optional"),
        "expected ILO-T026 mentioning Optional, got: {stderr}"
    );
}

// ── two-step nil-coalesce idiom still works (no regression) ──────────────
const TWO_STEP_SRC: &str = r#"f>n;m=mset mmap "k" 5;r=mget m "k";v=r??0;v"#;

fn check_two_step(engine: &str) {
    assert_eq!(run(engine, TWO_STEP_SRC, "f"), "5", "engine={engine}");
}

#[test]
fn mget_two_step_default_tree() {
    check_two_step("--run-tree");
}

#[test]
fn mget_two_step_default_vm() {
    check_two_step("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn mget_two_step_default_cranelift() {
    check_two_step("--run-cranelift");
}

// Two-step on missing key uses the default.
const TWO_STEP_MISS_SRC: &str = r#"f>n;m=mmap;r=mget m "k";v=r??42;v"#;

fn check_two_step_miss(engine: &str) {
    assert_eq!(run(engine, TWO_STEP_MISS_SRC, "f"), "42", "engine={engine}");
}

#[test]
fn mget_two_step_default_miss_tree() {
    check_two_step_miss("--run-tree");
}

#[test]
fn mget_two_step_default_miss_vm() {
    check_two_step_miss("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn mget_two_step_default_miss_cranelift() {
    check_two_step_miss("--run-cranelift");
}

// ── Result `!` propagation: cross-engine contract ───────────────────────
// `!` on a Result-returning builtin must short-circuit on Err the same way
// it does for user functions:
//   - Ok(v)  → extract v; subsequent statements see the inner value.
//   - Err(e) → return Value::Err(e) from the enclosing function, skipping
//              any remaining statements (including a tail `~v` wrap).
//
// Tree implements this via `RuntimeError.propagate_value`. The VM emits
// OP_ISOK / OP_RET / OP_UNWRAP at the call site; Cranelift inherits the
// behaviour because it consumes VM bytecode. All three engines must agree:
// before this fix `num!` and `dtfmt!` on VM/Cranelift skipped the guard
// and produced `Value::Ok(Value::Err(_))` on the Err branch.

// num! returns R n t.
const RESULT_BANG_OK_SRC: &str = r#"f>R n t;v=num! "42";~v"#;
const RESULT_BANG_ERR_SRC: &str = r#"f>R n t;v=num! "abc";~v"#;

fn check_result_ok(engine: &str) {
    // After the fix, every engine prints `~42` — num! unwraps to 42, then
    // the explicit `~v` wraps it as Ok(42). Previously VM/Cranelift printed
    // `~~42` because num! left the Ok wrap on.
    assert_eq!(
        run(engine, RESULT_BANG_OK_SRC, "f"),
        "~42",
        "engine={engine}"
    );
}

fn check_result_err(engine: &str) {
    // num! "abc" → Err("abc") propagates out of f before the `~v` wrap
    // runs. Entry-function Err contract (#255) means exit=1 with the err
    // on stderr.
    let stderr = run_err(engine, RESULT_BANG_ERR_SRC, "f");
    assert!(
        stderr.contains("abc"),
        "engine={engine}: expected err containing abc on stderr, got {stderr}"
    );
}

#[test]
fn result_bang_ok_tree() {
    check_result_ok("--run-tree");
}

#[test]
fn result_bang_ok_vm() {
    check_result_ok("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn result_bang_ok_cranelift() {
    check_result_ok("--run-cranelift");
}

#[test]
fn result_bang_err_tree() {
    check_result_err("--run-tree");
}

#[test]
fn result_bang_err_vm() {
    check_result_err("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn result_bang_err_cranelift() {
    check_result_err("--run-cranelift");
}

// ── dtfmt!: timestamp-out-of-range Err short-circuit ─────────────────────
// dtfmt returns R t t. A 14-digit epoch overflows chrono's range and
// surfaces as Err("dtfmt: timestamp out of range (...)"). Pre-fix, VM and
// Cranelift wrapped the Err in an Ok and emitted `~^dtfmt: …`. Tree
// propagated the Err out of f as exit=1.
const DTFMT_BANG_ERR_SRC: &str = r#"f>R t t;v=dtfmt! 99999999999999 "%Y";~v"#;

fn check_dtfmt_err(engine: &str) {
    let stderr = run_err(engine, DTFMT_BANG_ERR_SRC, "f");
    assert!(
        stderr.contains("dtfmt") && stderr.contains("out of range"),
        "engine={engine}: expected dtfmt err on stderr, got {stderr}"
    );
}

#[test]
fn dtfmt_bang_err_tree() {
    check_dtfmt_err("--run-tree");
}

#[test]
fn dtfmt_bang_err_vm() {
    check_dtfmt_err("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn dtfmt_bang_err_cranelift() {
    check_dtfmt_err("--run-cranelift");
}

// ── num! short-circuit skips subsequent statements ───────────────────────
// If num! propagates Err, the `~99` literal on the next line must not run.
const NUM_BANG_SHORTCIRCUIT_SRC: &str = r#"f>R n t;v=num! "abc";~99"#;

fn check_num_shortcircuit(engine: &str) {
    let stderr = run_err(engine, NUM_BANG_SHORTCIRCUIT_SRC, "f");
    assert!(
        stderr.contains("abc"),
        "engine={engine}: expected propagated err, got {stderr}"
    );
}

#[test]
fn num_bang_shortcircuit_tree() {
    check_num_shortcircuit("--run-tree");
}

#[test]
fn num_bang_shortcircuit_vm() {
    check_num_shortcircuit("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn num_bang_shortcircuit_cranelift() {
    check_num_shortcircuit("--run-cranelift");
}

// ── rd! / rdl! short-circuit on missing file ─────────────────────────────
// rd / rdl return R _ t. A path that doesn't exist must propagate as Err
// out of the enclosing function — same contract as num!. These cover the
// existing OP_RD / OP_RDL fast-paths (already correct pre-fix) so we don't
// regress them while wiring up num! and dtfmt!.
const RD_BANG_SRC: &str = r#"f>R t t;v=rd! "/no/such/path/ilo-test";~v"#;
const RDL_BANG_SRC: &str = r#"f>R (L t) t;v=rdl! "/no/such/path/ilo-test";~v"#;

fn check_rd_err(engine: &str) {
    let stderr = run_err(engine, RD_BANG_SRC, "f");
    assert!(
        stderr.to_lowercase().contains("no such")
            || stderr.contains("not found")
            || stderr.contains("/no/such/path"),
        "engine={engine}: expected file-not-found err, got {stderr}"
    );
}

fn check_rdl_err(engine: &str) {
    let stderr = run_err(engine, RDL_BANG_SRC, "f");
    assert!(
        stderr.to_lowercase().contains("no such")
            || stderr.contains("not found")
            || stderr.contains("/no/such/path"),
        "engine={engine}: expected file-not-found err, got {stderr}"
    );
}

#[test]
fn rd_bang_err_tree() {
    check_rd_err("--run-tree");
}

#[test]
fn rd_bang_err_vm() {
    check_rd_err("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn rd_bang_err_cranelift() {
    check_rd_err("--run-cranelift");
}

#[test]
fn rdl_bang_err_tree() {
    check_rdl_err("--run-tree");
}

#[test]
fn rdl_bang_err_vm() {
    check_rdl_err("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn rdl_bang_err_cranelift() {
    check_rdl_err("--run-cranelift");
}
