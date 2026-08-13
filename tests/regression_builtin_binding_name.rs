// Regression: a local binding whose name collides with a builtin shadows
// that builtin in VALUE position, while CALL position still dispatches the
// builtin. Python's rule, and the one 69565d44 introduced.
//
// History. The original persona report (2026-05-16 pdf-analyst, friction #6)
// was that `flat=cat ls " "` then `spl flat ". "` silently bound `flat` and
// then parsed the use site as a 0-arg call to the `flat` BUILTIN, surfacing a
// misleading `ILO-T006 arity mismatch: 'flat' expects 1 args, got 0`. The
// first fix rejected such bindings outright at parse time (ILO-P011), and
// this suite pinned that rejection.
//
// 69565d44 replaced rejection with shadowing, because reserved-name collisions
// were the #1 benchmark failure category and the repair loop could not fix
// them: renaming is not something the model reliably infers. The bug the
// original report describes is still fixed, just differently - the use site
// now resolves to the local instead of mis-dispatching. These tests assert
// that resolution directly, so the T006 mis-dispatch cannot come back.
//
// `fld` keeps its hard rejection (see the tail of this file): it is reserved
// for the fold builtin with a dedicated message, not a general shadowable name.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str) -> (bool, String, String) {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// In-function (`parse_stmt`) binding form: `name=expr` inside a function body.
fn check_in_fn_binding(engine: &str, name: &str) {
    let src = format!("main>n;{name}=5;{name}");
    let (ok, stdout, stderr) = run(engine, &src, "main");
    assert!(
        ok,
        "engine={engine} name={name}: builtin-named binding should be accepted, stderr={stderr}"
    );
    assert_eq!(
        stdout.trim(),
        "5",
        "engine={engine} name={name}: use site should resolve to the local, stderr={stderr}"
    );
    // The bug from the original report: the use site must not dispatch to the
    // shadowed builtin and report an arity mismatch against it.
    assert!(
        !stderr.contains("ILO-T006"),
        "engine={engine} name={name}: builtin mis-dispatch returned, stderr={stderr}"
    );
}

// Top-level (`parse_decl`) binding form: `name=expr` outside any function.
// Script mode wraps the bare statements into main, so no explicit `main` here
// (that shape is ILO-P104 and is covered by the script-mode suite).
fn check_top_level_binding(engine: &str, name: &str) {
    let src = format!("{name}=5\nprnt {name}");
    let (ok, stdout, stderr) = run(engine, &src, "main");
    assert!(
        ok,
        "engine={engine} name={name}: top-level builtin-named binding should be accepted, stderr={stderr}"
    );
    assert_eq!(
        stdout.trim(),
        "5",
        "engine={engine} name={name}: top-level use site should resolve to the local, stderr={stderr}"
    );
}

// Builtin names a persona is likely to reach for as a local-binding name.
// Mix of the rerun3-cited `flat`, list/map builtins (`map`, `flt`, `frq`,
// `cat`, `len`), and short-name builtins (`hd`, `tl`, `at`, `ord`) that
// collide with natural single-letter abbreviations.
const BINDING_NAMES: &[&str] = &[
    "flat", "frq", "map", "flt", "cat", "len", "hd", "tl", "at", "ord", "srt", "sum",
];

#[test]
fn builtin_binding_accepted_in_fn_vm() {
    for name in BINDING_NAMES {
        check_in_fn_binding("--vm", name);
    }
}

#[test]
#[cfg(feature = "cranelift")]
fn builtin_binding_accepted_in_fn_cranelift() {
    for name in BINDING_NAMES {
        check_in_fn_binding("--jit", name);
    }
}

#[test]
fn builtin_binding_accepted_top_level_vm() {
    for name in BINDING_NAMES {
        check_top_level_binding("--vm", name);
    }
}

#[test]
#[cfg(feature = "cranelift")]
fn builtin_binding_accepted_top_level_cranelift() {
    for name in BINDING_NAMES {
        check_top_level_binding("--jit", name);
    }
}

// The exact pdf-analyst rerun3 repro, now asserting the shadow resolves
// rather than that the binding is rejected.
const FLAT_REPRO: &str = "main>n;flat=5;flat";

fn check_flat_repro(engine: &str) {
    let (ok, stdout, stderr) = run(engine, FLAT_REPRO, "main");
    assert!(ok, "engine={engine}: expected success, stderr={stderr}");
    assert_eq!(stdout.trim(), "5", "engine={engine}: stderr={stderr}");
    assert!(
        !stderr.contains("ILO-T006"),
        "engine={engine}: builtin mis-dispatch returned, stderr={stderr}"
    );
}

#[test]
fn flat_repro_vm() {
    check_flat_repro("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn flat_repro_cranelift() {
    check_flat_repro("--jit");
}

// Shadowing is value-position only: with a local `len` in scope, `len xs` in
// call position still dispatches the builtin. This is the half of the rule an
// over-eager future change is most likely to break.
fn check_call_position_still_dispatches_builtin(engine: &str) {
    let src = "main>n;len=5;xs=[1,2,3];r=len xs;+r len";
    let (ok, stdout, stderr) = run(engine, src, "main");
    assert!(ok, "engine={engine}: expected success, stderr={stderr}");
    // len xs = 3 (builtin), + local len (5) = 8
    assert_eq!(stdout.trim(), "8", "engine={engine}: stderr={stderr}");
}

#[test]
fn call_position_dispatches_builtin_vm() {
    check_call_position_still_dispatches_builtin("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn call_position_dispatches_builtin_cranelift() {
    check_call_position_still_dispatches_builtin("--jit");
}

// `fld` is exempt from shadowing and keeps its dedicated ILO-P011 message.
fn check_fld_keeps_specific_message(engine: &str) {
    let (ok, _stdout, stderr) = run(engine, "main>n;fld=5;fld", "main");
    assert!(!ok, "engine={engine}: expected parse failure");
    assert!(
        stderr.contains("ILO-P011"),
        "engine={engine}: missing ILO-P011, stderr={stderr}"
    );
    assert!(
        stderr.contains("`fld` is reserved for the fold builtin"),
        "engine={engine}: expected fld-specific message, stderr={stderr}"
    );
}

#[test]
fn fld_keeps_specific_message_vm() {
    check_fld_keeps_specific_message("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn fld_keeps_specific_message_cranelift() {
    check_fld_keeps_specific_message("--jit");
}
