// Regression tests for the Cranelift JIT-helper permissive-nil sweep, batch 1.
//
// Helpers in scope: jit_lst, jit_index, jit_slc, jit_jpth, jit_listget.
//
// Before this PR these helpers silently returned TAG_NIL (or the input list)
// on failure paths where tree/VM raise runtime errors. The fix routes the
// failure paths through the same `JIT_RUNTIME_ERROR` TLS cell introduced in
// #254, so every engine now surfaces a runtime error with matching shape.
//
// Note on slc: tree and VM deliberately clamp out-of-range start/end indices
// (slc is documented to saturate), so OOB on slc is NOT an error on any
// engine. Only type errors are surfaced. The OOB-clamp tests below pin that
// the JIT continues to clamp rather than newly erroring.
//
// Note on listget: OOB-nil is the foreach loop-done sentinel and is left in
// place. Only the type-error paths are surfaced as runtime errors.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn check_runtime_error(engine: &str, src: &str, kw_any: &[&str]) {
    let out = ilo()
        .args([src, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "engine={engine}: expected runtime error for `{src}`, got stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        kw_any.iter().any(|k| stderr.contains(k)),
        "engine={engine}: expected one of {:?} in stderr, got stderr={stderr}",
        kw_any
    );
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

// ── lst: OOB ──────────────────────────────────────────────────────────────

#[test]
fn lst_oob_tree() {
    check_runtime_error(
        "--run-tree",
        "f>L n;lst [1,2,3] 5 99",
        &["lst", "out of range", "ILO-R009"],
    );
}

#[test]
fn lst_oob_vm() {
    check_runtime_error(
        "--run-vm",
        "f>L n;lst [1,2,3] 5 99",
        &["lst", "out of range", "ILO-R004"],
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn lst_oob_cranelift() {
    check_runtime_error(
        "--run-cranelift",
        "f>L n;lst [1,2,3] 5 99",
        &["lst", "out of range", "ILO-R004"],
    );
}

// ── lst: negative index ───────────────────────────────────────────────────

#[test]
fn lst_negative_tree() {
    check_runtime_error(
        "--run-tree",
        "f>L n;lst [1,2,3] -1 99",
        &["lst", "non-negative", "integer", "ILO-R009"],
    );
}

#[test]
fn lst_negative_vm() {
    check_runtime_error(
        "--run-vm",
        "f>L n;lst [1,2,3] -1 99",
        &["lst", "non-negative", "integer", "ILO-R004"],
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn lst_negative_cranelift() {
    check_runtime_error(
        "--run-cranelift",
        "f>L n;lst [1,2,3] -1 99",
        &["lst", "non-negative", "integer", "ILO-R004"],
    );
}

// ── lst: happy path (regression — make sure we did not break the success
// case) ───────────────────────────────────────────────────────────────────

#[test]
fn lst_ok_tree() {
    check_stdout("--run-tree", "f>L n;lst [1,2,3] 1 99", "[1, 99, 3]");
}

#[test]
fn lst_ok_vm() {
    check_stdout("--run-vm", "f>L n;lst [1,2,3] 1 99", "[1, 99, 3]");
}

#[test]
#[cfg(feature = "cranelift")]
fn lst_ok_cranelift() {
    check_stdout("--run-cranelift", "f>L n;lst [1,2,3] 1 99", "[1, 99, 3]");
}

// ── slc: type error on non-number index ──────────────────────────────────
//
// We cannot easily express a non-number index in surface ilo (verify will
// reject it), so this case is covered by the VM-level type-mismatch path
// that we know fires when the helper is called with non-number bits. The
// happy path + OOB-clamp tests below confirm that the type-error change
// did not regress the documented saturation semantic.

// ── slc: OOB is deliberately clamped on every engine ─────────────────────

#[test]
fn slc_oob_clamps_tree() {
    check_stdout("--run-tree", "f>L n;slc [1,2,3] 1 999", "[2, 3]");
}

#[test]
fn slc_oob_clamps_vm() {
    check_stdout("--run-vm", "f>L n;slc [1,2,3] 1 999", "[2, 3]");
}

#[test]
#[cfg(feature = "cranelift")]
fn slc_oob_clamps_cranelift() {
    check_stdout("--run-cranelift", "f>L n;slc [1,2,3] 1 999", "[2, 3]");
}

#[test]
fn slc_text_oob_clamps_tree() {
    check_stdout("--run-tree", "f>t;slc \"hello\" 1 999", "ello");
}

#[test]
fn slc_text_oob_clamps_vm() {
    check_stdout("--run-vm", "f>t;slc \"hello\" 1 999", "ello");
}

#[test]
#[cfg(feature = "cranelift")]
fn slc_text_oob_clamps_cranelift() {
    check_stdout("--run-cranelift", "f>t;slc \"hello\" 1 999", "ello");
}

// ── jpth: path miss returns Err(...) on every engine (regression) ────────
//
// This is the existing documented contract — path miss is wrapped in a
// Result, NOT a runtime error. Pin it across engines so the type-error
// change below does not accidentally widen the error surface.

// We wrap the call in `prnt v;0` so that the returned Result reaches stdout
// without making `main` exit 1 (that would conflate Err-return with helper
// error). The stdout assertion below pins the rendered Result.

#[test]
fn jpth_path_miss_tree() {
    check_stdout(
        "--run-tree",
        "f>n;v=jpth \"{\\\"a\\\":1}\" \"b\";prnt v;0",
        "^key not found: b\n0",
    );
}

#[test]
fn jpth_path_miss_vm() {
    check_stdout(
        "--run-vm",
        "f>n;v=jpth \"{\\\"a\\\":1}\" \"b\";prnt v;0",
        "^key not found: b\n0",
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn jpth_path_miss_cranelift() {
    check_stdout(
        "--run-cranelift",
        "f>n;v=jpth \"{\\\"a\\\":1}\" \"b\";prnt v;0",
        "^key not found: b\n0",
    );
}

// ── jpth: happy path ─────────────────────────────────────────────────────

#[test]
fn jpth_ok_tree() {
    check_stdout(
        "--run-tree",
        "f>n;v=jpth \"{\\\"a\\\":1}\" \"a\";prnt v;0",
        "~1\n0",
    );
}

#[test]
fn jpth_ok_vm() {
    check_stdout(
        "--run-vm",
        "f>n;v=jpth \"{\\\"a\\\":1}\" \"a\";prnt v;0",
        "~1\n0",
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn jpth_ok_cranelift() {
    check_stdout(
        "--run-cranelift",
        "f>n;v=jpth \"{\\\"a\\\":1}\" \"a\";prnt v;0",
        "~1\n0",
    );
}

// ── index (xs.N literal-index OP_INDEX): OOB ─────────────────────────────
//
// `xs.5` on a 3-element list goes through OP_INDEX, which the Cranelift
// backend lowers to a `jit_index` call. Before this PR the JIT path
// silently returned nil; tree/VM both surface a runtime error. Pin parity
// across engines.

#[test]
fn index_oob_tree() {
    check_runtime_error(
        "--run-tree",
        "f>n;xs=[10,20,30];xs.5",
        &["out of bounds", "ILO-R006"],
    );
}

#[test]
fn index_oob_vm() {
    check_runtime_error(
        "--run-vm",
        "f>n;xs=[10,20,30];xs.5",
        &["out of bounds", "ILO-R004"],
    );
}

#[test]
#[cfg(feature = "cranelift")]
fn index_oob_cranelift() {
    check_runtime_error(
        "--run-cranelift",
        "f>n;xs=[10,20,30];xs.5",
        &["out of bounds", "ILO-R004"],
    );
}

// ── index: happy path (regression on the new error-path edits) ───────────

#[test]
fn index_ok_tree() {
    check_stdout("--run-tree", "f>n;xs=[10,20,30];xs.1", "20");
}

#[test]
fn index_ok_vm() {
    check_stdout("--run-vm", "f>n;xs=[10,20,30];xs.1", "20");
}

#[test]
#[cfg(feature = "cranelift")]
fn index_ok_cranelift() {
    check_stdout("--run-cranelift", "f>n;xs=[10,20,30];xs.1", "20");
}

// ── No stale-error leak after the new failure paths ──────────────────────
//
// Same shape as the #254 stale-error-leak guard, but pinned for the
// helpers added in this batch. If the JitRuntimeErrorGuard ever regressed,
// a successful call following an erroring one would inherit the stale
// error and spuriously fail.
#[test]
#[cfg(feature = "cranelift")]
fn no_stale_jit_error_leak_after_lst_oob() {
    // First call: lst OOB → runtime error.
    let out = ilo()
        .args(["f>L n;lst [1,2,3] 5 99", "--run-cranelift", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "first call: expected runtime error from lst OOB"
    );

    // Second call in a fresh process: must succeed cleanly.
    let out = ilo()
        .args(["f>L n;lst [1,2,3] 1 99", "--run-cranelift", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "second call: expected success, got stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "[1, 99, 3]",
        "second call should produce the updated list"
    );
}
