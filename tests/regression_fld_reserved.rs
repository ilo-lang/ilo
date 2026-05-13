// Regression: `fld` used as a binding name must surface the friendly
// ILO-P011 reserved-word error, not a misleading ILO-T006 arity mismatch
// from the fold builtin. Mirrors the `cnt`/`brk` handling from commit
// 8928635. Personas reach for `fld` as a natural variable name (field /
// fold / folder) and previously paid the retry tax on a cryptic error.
//
// The fix lives in the parser, so all engines surface the same error.
// Tests confirm each engine's CLI path emits ILO-P011 with the right
// wording.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str) -> (bool, String) {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// `fld=5` at top level inside a function body.
const FLD_BINDING_IN_BODY: &str = "f>n;fld=5;fld";

fn check_fld_binding(engine: &str) {
    let (ok, stderr) = run(engine, FLD_BINDING_IN_BODY, "f");
    assert!(!ok, "engine={engine}: expected parse failure");
    assert!(
        stderr.contains("ILO-P011"),
        "engine={engine}: missing ILO-P011, stderr={stderr}"
    );
    assert!(
        stderr.contains("`fld` is reserved for the fold builtin"),
        "engine={engine}: missing friendly message, stderr={stderr}"
    );
    assert!(
        stderr.contains("field") || stderr.contains("folder"),
        "engine={engine}: hint should suggest field/folder, stderr={stderr}"
    );
    // Should not cascade into the verifier's misleading arity error.
    assert!(
        !stderr.contains("ILO-T006"),
        "engine={engine}: arity cascade leaked, stderr={stderr}"
    );
    assert!(
        !stderr.contains("arity mismatch"),
        "engine={engine}: arity cascade leaked, stderr={stderr}"
    );
}

#[test]
fn fld_binding_in_body_tree() {
    check_fld_binding("--run-tree");
}

#[test]
fn fld_binding_in_body_vm() {
    check_fld_binding("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn fld_binding_in_body_cranelift() {
    check_fld_binding("--run-cranelift");
}

// `fld=5` inside a loop body, the natural shape a persona writes when
// accumulating a fold-style value across iterations.
const FLD_BINDING_IN_LOOP: &str = "f a:n>n;@i 0..a{fld=i};1";

fn check_fld_binding_loop(engine: &str) {
    let (ok, stderr) = run(engine, FLD_BINDING_IN_LOOP, "f");
    assert!(!ok, "engine={engine}: expected parse failure");
    assert!(
        stderr.contains("ILO-P011"),
        "engine={engine}: missing ILO-P011, stderr={stderr}"
    );
    assert!(
        stderr.contains("`fld` is reserved for the fold builtin"),
        "engine={engine}: missing friendly message, stderr={stderr}"
    );
    assert!(
        !stderr.contains("ILO-T006"),
        "engine={engine}: arity cascade leaked, stderr={stderr}"
    );
}

#[test]
fn fld_binding_in_loop_tree() {
    check_fld_binding_loop("--run-tree");
}

#[test]
fn fld_binding_in_loop_vm() {
    check_fld_binding_loop("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn fld_binding_in_loop_cranelift() {
    check_fld_binding_loop("--run-cranelift");
}

// Sanity: `fld` as the fold builtin still works after the fix.
#[test]
fn fld_as_builtin_still_works() {
    let out = ilo()
        .args([
            "add x:n y:n>n;+x y;f>n;fld add [1 2 3 4] 0",
            "--run-tree",
            "f",
        ])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "fld builtin broken: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("10"), "expected 10, got: {stdout}");
}
