// Regression tests for ILO-T004 specialised hint on `name.N` where
// `name` is unbound.
//
// Source: pending #5bc in /Users/dan/code/ilo_feedback/pending.md.
//
// Agents reach for `tup.0` / `pair.0` tuple syntax after `zip xs ys`,
// not realising zip returns `L (L n)` and ilo has no tuple type. The
// previous diagnostic was a bare "undefined variable 'tup'" with at
// best a closest-match `did you mean` hint — neither pointed at the
// correct `at <name> <N>` shape.
//
// The fix intercepts `Expr::Index { object: Ref(name), index: N, .. }`
// at the verifier, checks whether `name` is bound in scope / functions /
// builtins, and if not emits ILO-T004 with a hint naming `at <name> <N>`
// and explaining that `zip` returns a list of lists, not tuples.
//
// Cross-engine: the diagnostic fires in the verifier before any backend
// runs, so output is identical across vm and jit. We still exercise both
// to confirm no engine re-verifies around the check.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(name: &str, src: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "ilo_tup_at_hint_{name}_{}_{n}.ilo",
        std::process::id()
    ));
    std::fs::write(&path, src).expect("write src");
    path
}

fn run_capture(engine: &str, src: &str, entry: &str) -> (bool, String, String) {
    let path = write_src(entry, src);
    let mut cmd = ilo();
    cmd.arg(&path).arg(engine).arg(entry);
    let out = cmd.output().expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn check_capture(src: &str) -> (bool, String) {
    let path = write_src("check", src);
    let mut cmd = ilo();
    cmd.arg("check").arg(&path);
    let out = cmd.output().expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), combined)
}

// --- Repro: bare `tup.0` / `tup.1` with `tup` never bound -------------

const TUP_NEVER_BOUND: &str = "f>n;+tup.0 tup.1";

fn check_tup_never_bound(engine: &str) {
    let (ok, _stdout, stderr) = run_capture(engine, TUP_NEVER_BOUND, "f");
    assert!(
        !ok,
        "{engine}: `tup.0` with unbound `tup` must reject in verifier"
    );
    assert!(
        stderr.contains("ILO-T004"),
        "{engine}: expected ILO-T004, got: {stderr}"
    );
    assert!(
        stderr.contains("at tup 0"),
        "{engine}: hint should suggest `at tup 0`, got: {stderr}"
    );
    assert!(
        stderr.contains("at tup 1"),
        "{engine}: hint should suggest `at tup 1` for the second index, got: {stderr}"
    );
    assert!(
        stderr.contains("no tuple type"),
        "{engine}: hint should mention ilo has no tuple type, got: {stderr}"
    );
    assert!(
        stderr.contains("L (L n)") || stderr.contains("list of lists"),
        "{engine}: hint should mention zip returns L (L n) / list of lists, got: {stderr}"
    );
}

// --- Repro: `pair.0` shaped the same way -----------------------------

const PAIR_NEVER_BOUND: &str = "f>n;pair.0";

fn check_pair_never_bound(engine: &str) {
    let (ok, _stdout, stderr) = run_capture(engine, PAIR_NEVER_BOUND, "f");
    assert!(
        !ok,
        "{engine}: `pair.0` with unbound `pair` must reject in verifier"
    );
    assert!(
        stderr.contains("ILO-T004"),
        "{engine}: expected ILO-T004, got: {stderr}"
    );
    assert!(
        stderr.contains("at pair 0"),
        "{engine}: hint should suggest `at pair 0`, got: {stderr}"
    );
}

// --- Negative: a bound `pair:L n` keeps existing field-access behaviour --
//
// When `pair` is bound (e.g. as a lambda param of type `L n`), `pair.0`
// is valid sugar for list indexing and the program runs successfully.
// The new hint must not interfere with the happy path.

const PAIR_BOUND_OK: &str = "g pair:L n>n;+pair.0 pair.1\n\
                             f>L n;\n\
                               xs=[1 2 3];\n\
                               ys=[10 20 30];\n\
                               zs=zip xs ys;\n\
                               map g zs";

fn check_pair_bound_runs(engine: &str) {
    let (ok, stdout, stderr) = run_capture(engine, PAIR_BOUND_OK, "f");
    assert!(
        ok,
        "{engine}: bound `pair:L n` with `pair.0` must run cleanly. stderr={stderr}"
    );
    assert!(
        stdout.contains("[11, 22, 33]"),
        "{engine}: expected dot-pair sum, got stdout: {stdout}"
    );
}

// --- Negative: an unbound name without `.N` indexing keeps the regular
// closest-match hint (no spurious "at" suggestion when the user isn't
// indexing). Guards against the hint leaking into unrelated diagnostics.

const PLAIN_UNBOUND: &str = "f>n;+tup 1";

fn check_plain_unbound_no_at_hint(engine: &str) {
    let (_ok, _stdout, stderr) = run_capture(engine, PLAIN_UNBOUND, "f");
    assert!(
        stderr.contains("ILO-T004"),
        "{engine}: expected ILO-T004 for plain unbound, got: {stderr}"
    );
    assert!(
        !stderr.contains("at tup"),
        "{engine}: plain unbound `tup` must NOT carry an `at tup` hint, got: {stderr}"
    );
}

// --- Sanity check via `ilo check` (no engine) -----------------------

#[test]
fn tup_at_hint_via_ilo_check() {
    let (ok, combined) = check_capture(TUP_NEVER_BOUND);
    assert!(!ok, "`ilo check` must reject");
    assert!(
        combined.contains("ILO-T004"),
        "expected ILO-T004 from `ilo check`, got: {combined}"
    );
    assert!(
        combined.contains("at tup 0"),
        "`ilo check` should carry the new hint, got: {combined}"
    );
}

fn check_all(engine: &str) {
    check_tup_never_bound(engine);
    check_pair_never_bound(engine);
    check_pair_bound_runs(engine);
    check_plain_unbound_no_at_hint(engine);
}

#[test]
fn tup_at_hint_vm() {
    check_all("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn tup_at_hint_cranelift() {
    check_all("--jit");
}
