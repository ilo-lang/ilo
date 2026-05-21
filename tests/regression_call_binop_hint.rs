// Regression tests for the ILO-T005 call-vs-binop diagnostic hint.
//
// Source: n-body persona rerun 2026-05-20, pending #5au.
//
// In ilo, whitespace-juxtaposition is the call syntax. Any bare name
// followed by another token in an expression is parsed as a call, so:
//
//     dx=xj 0-xi
//
// parses as `dx = (xj 0) - xi` — a call to `xj` with argument `0`. When
// `xj` is a number (the common case: a parameter), verification fails
// with ILO-T005 because numbers aren't callable.
//
// The fix is diagnostic-only: when the call-on-non-fn pattern has a
// single simple-shaped argument (numeric literal or bare ref), the
// suggestion explains the call-vs-binop confusion and points at the
// prefix-operator alternatives.
//
// Cross-engine: the diagnostic fires in the verifier, before any engine
// runs. Output is identical across vm and cranelift. We still exercise
// both to confirm no engine re-verifies around the new hint.

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
        "ilo_call_binop_hint_{name}_{}_{n}.ilo",
        std::process::id()
    ));
    std::fs::write(&path, src).expect("write src");
    path
}

fn run_capture(engine: &str, src: &str, entry: &str, args: &[&str]) -> (bool, String, String) {
    let path = write_src(entry, src);
    let mut cmd = ilo();
    cmd.arg(&path).arg(engine).arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

// --- Repro: the n-body persona shape ----------------------------------
//
// `dx=xj 0-xi` parses as `dx=(xj 0)-xi`, hitting ILO-T005 because xj is
// a number. The hint should explain the call-vs-binop confusion and
// point at the prefix-operator fix.

const PERSONA_REPRO: &str = "t xi:n xj:n>n;dx=xj 0-xi;dx";

fn check_persona_repro(engine: &str) {
    let (ok, _stdout, stderr) = run_capture(engine, PERSONA_REPRO, "t", &["1", "5"]);
    assert!(
        !ok,
        "{engine}: call-on-non-fn must reject (got success). stderr={stderr}"
    );
    assert!(
        stderr.contains("ILO-T005"),
        "{engine}: expected ILO-T005, got: {stderr}"
    );
    // The tailored hint pins the call-vs-binop framing.
    assert!(
        stderr.contains("whitespace-juxtaposition"),
        "{engine}: expected tailored call-vs-binop hint, got: {stderr}"
    );
    // Prefix-operator alternatives are surfaced explicitly.
    assert!(
        stderr.contains("+xj") && stderr.contains("-xj"),
        "{engine}: expected prefix-op alternatives in hint, got: {stderr}"
    );
    // And the long-form pointer for ilo --explain.
    assert!(
        stderr.contains("--explain"),
        "{engine}: hint should point at --explain, got: {stderr}"
    );
}

#[test]
fn persona_repro_vm() {
    check_persona_repro("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn persona_repro_cranelift() {
    check_persona_repro("--jit");
}

// --- Variant: ref-as-arg also triggers --------------------------------
//
// `dx=xj k` where xj is a number and k is a number bound: same misparse
// in spirit, the hint should still fire.

const REF_AS_ARG: &str = "t xi:n xj:n>n;k=2;dx=xj k;dx";

fn check_ref_as_arg(engine: &str) {
    let (ok, _stdout, stderr) = run_capture(engine, REF_AS_ARG, "t", &["1", "5"]);
    assert!(!ok, "{engine}: must reject. stderr={stderr}");
    assert!(
        stderr.contains("ILO-T005"),
        "{engine}: expected ILO-T005, got: {stderr}"
    );
    assert!(
        stderr.contains("whitespace-juxtaposition"),
        "{engine}: expected tailored hint for ref-as-arg shape, got: {stderr}"
    );
    assert!(
        stderr.contains("`xj k`") || stderr.contains("xj k"),
        "{engine}: hint should reference the call shape, got: {stderr}"
    );
}

#[test]
fn ref_as_arg_vm() {
    check_ref_as_arg("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn ref_as_arg_cranelift() {
    check_ref_as_arg("--jit");
}

// --- Negative: the canonical workaround compiles and runs cleanly ----

const CANONICAL_FIX: &str = "t xi:n xj:n>n;nxi=0-xi;+xj nxi";

fn check_canonical_fix(engine: &str) {
    let (ok, stdout, stderr) = run_capture(engine, CANONICAL_FIX, "t", &["1", "5"]);
    assert!(
        ok,
        "{engine}: canonical fix must run cleanly. stderr={stderr}"
    );
    // xj + (0 - xi) = 5 + (0 - 1) = 4
    assert!(
        stdout.trim().contains('4'),
        "{engine}: expected result 4, got stdout: {stdout}"
    );
}

#[test]
fn canonical_fix_vm() {
    check_canonical_fix("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn canonical_fix_cranelift() {
    check_canonical_fix("--jit");
}

// --- Negative: existing "did you mean" hint still fires for typos -----
//
// If the callee is undefined AND a close-match candidate exists, the
// fuzzy-match hint wins. The binop hint is only a fallback when there's
// no good "did you mean" candidate.

const TYPO_KEEPS_DID_YOU_MEAN: &str = "double x:n>n;*x 2\nf x:n>n;doublee x";

#[test]
fn typo_keeps_did_you_mean() {
    let path = write_src("typo", TYPO_KEEPS_DID_YOU_MEAN);
    let mut cmd = ilo();
    cmd.arg("check").arg(&path);
    let out = cmd.output().expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("ILO-T005"),
        "expected ILO-T005 for typo, got: {combined}"
    );
    assert!(
        combined.contains("did you mean") || combined.contains("double"),
        "fuzzy-match hint should still fire for typos, got: {combined}"
    );
}

// --- Sanity: ilo --explain ILO-T005 surfaces the new gotcha section --

#[test]
fn explain_t005_mentions_call_vs_binop() {
    let mut cmd = ilo();
    cmd.arg("--explain").arg("ILO-T005");
    let out = cmd.output().expect("failed to run ilo");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("call vs binary-op") || combined.contains("call-vs-binop"),
        "expected --explain output to mention the gotcha, got: {combined}"
    );
    assert!(
        combined.contains("whitespace-juxtaposition") || combined.contains("Whitespace"),
        "expected --explain output to explain the juxtaposition mechanic, got: {combined}"
    );
}
