// Regression tests for ILO-P102: top-level chained bindings written
// without a `main>_;` wrapper.
//
// Source: k-means rerun (caused 600s watchdog timeout) and linear-
// regression rerun in /Users/dan/code/ilo_assessment_feedback.md.
// Pending #5aj.
//
// When a persona writes:
//
//     pts=gen-pts; cs0=[[4.8 4.9]...]; cs1=iter cs0 pts; ...; prnt cs2
//
// at the top level without wrapping in `main>_;`, the parser either:
//   (a) emits a bare `ILO-P003 expected '>'` with no actionable fix, or
//   (b) when a previous `name>type;body` decl is in scope, the trailing
//       bare ident on that decl's body greedily eats the first ident
//       from the next un-indented line as a call argument, and the
//       whole chain gets slurped into the prior fn's body — producing
//       a wall of ILO-T005 cascades anchored on the wrong function.
//
// The fix is two-pronged in the parser:
//   1. `can_start_operand` refuses to cross a top-level newline into
//      an `Ident = ...` binding shape, so the slurp can't start.
//   2. `parse_decl` recognises a bare `Ident = ...` at the top level
//      and emits ILO-P102 with the `main>_;` wrapper hint.
//
// Cross-engine: ILO-P102 fires in the parser, so output is identical
// across vm and jit. We still exercise both to confirm no engine
// re-parses around the new check.

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
        "ilo_top_chain_p102_{name}_{}_{n}.ilo",
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

fn check_capture(_engine: &str, src: &str) -> (bool, String) {
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

// --- Repro 1: bare top-level chain at the start of the file ------------

const BARE_TOP_CHAIN: &str = "pts=[1 2 3];cs0=[4 5 6];cs1=pts;prnt cs1";

fn check_bare_top_chain(engine: &str) {
    let (ok, _stdout, stderr) = run_capture(engine, BARE_TOP_CHAIN, "main");
    assert!(
        !ok,
        "{engine}: top-level chain without main wrapper must reject at parse time"
    );
    assert!(
        stderr.contains("ILO-P102"),
        "{engine}: expected ILO-P102, got: {stderr}"
    );
    assert!(
        stderr.contains("main>_"),
        "{engine}: diagnostic should suggest `main>_;` wrapper, got: {stderr}"
    );
}

// --- Repro 2: chain slurped into a prior fn body (the k-means shape) ----
//
// The exact misparse: `gen-pts>L(L n);[[2.0 3.0][8.0 8.0]]` is a real
// fn, then `iter cs:L(L n) pts:L(L n)>L(L n);cs` is a real fn whose
// body is bare `cs`. Without the fix, `cs` greedily eats `pts` from
// line 3 as a call argument, producing a `(cs pts) = gen-pts` binop
// soup and slurping the rest of line 3 into `iter`'s body.

const SLURP_INTO_PRIOR_FN: &str = "gen-pts>L(L n);[[2.0 3.0][8.0 8.0]]\n\
                                   iter cs:L(L n) pts:L(L n)>L(L n);cs\n\
                                   pts=gen-pts;cs0=[[4.8 4.9][6.2 7.1]];cs1=iter cs0 pts;cs2=iter cs1 pts;prnt cs2";

fn check_slurp_into_prior_fn(engine: &str) {
    let (ok, _stdout, stderr) = run_capture(engine, SLURP_INTO_PRIOR_FN, "main");
    assert!(
        !ok,
        "{engine}: slurped top-level chain must reject at parse time"
    );
    assert!(
        stderr.contains("ILO-P102"),
        "{engine}: expected ILO-P102 (not a T005 cascade), got: {stderr}"
    );
    // The misparse used to produce multiple ILO-T005 errors anchored on
    // the wrong function. The fix collapses to a single P102 — verify
    // we don't get a T005 cascade.
    assert!(
        !stderr.contains("ILO-T005"),
        "{engine}: ILO-P102 should preempt the T005 cascade, got: {stderr}"
    );
}

// --- Workaround: wrap in `main>_;` makes the same chain valid -----------

const WITH_MAIN_WRAPPER: &str = "main>_;pts=[1 2 3];cs0=[4 5 6];cs1=pts;prnt cs1";

fn check_main_wrapper_runs(engine: &str) {
    let (ok, stdout, stderr) = run_capture(engine, WITH_MAIN_WRAPPER, "main");
    assert!(
        ok,
        "{engine}: same chain wrapped in main>_; must run cleanly. stderr={stderr}"
    );
    assert!(
        stdout.contains("[1, 2, 3]"),
        "{engine}: main wrapper should print cs1, got stdout: {stdout}"
    );
}

// --- Negative: a normal file starting with a fn decl is unaffected ------

const NORMAL_FN_DECL: &str = "f x:n>n;+x 1\nmain>_;prnt(f 41)";

fn check_normal_fn_decl_unaffected(engine: &str) {
    let (ok, stdout, stderr) = run_capture(engine, NORMAL_FN_DECL, "main");
    assert!(
        ok,
        "{engine}: normal fn decl + main must run. stderr={stderr}"
    );
    assert!(
        stdout.contains("42"),
        "{engine}: expected '42' in output, got: {stdout}"
    );
}

// --- Negative: a builtin-shadowing name keeps its precise ILO-P011 hint ----
//
// `map=...` at the top level still hits the existing ILO-P011 guard for
// builtin shadowing — the P102 generic guard must not eclipse it.

const MAP_SHADOW: &str = "map=[1 2 3];prnt map";

fn check_builtin_shadow_keeps_p011(engine: &str) {
    let (_ok, stdout, stderr) = run_capture(engine, MAP_SHADOW, "main");
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("ILO-P011"),
        "{engine}: `map=` should keep its builtin-shadow ILO-P011 hint, got: {combined}"
    );
}

// --- Sanity check via `ilo check` (no engine), exercising the parser only -

#[test]
fn p102_via_ilo_check_no_engine() {
    let (ok, combined) = check_capture("--vm", BARE_TOP_CHAIN);
    assert!(!ok, "`ilo check` must reject top-level chain");
    assert!(
        combined.contains("ILO-P102"),
        "expected ILO-P102 from `ilo check`, got: {combined}"
    );
}

fn check_all(engine: &str) {
    check_bare_top_chain(engine);
    check_slurp_into_prior_fn(engine);
    check_main_wrapper_runs(engine);
    check_normal_fn_decl_unaffected(engine);
    check_builtin_shadow_keeps_p011(engine);
}

#[test]
fn top_level_chain_hint_vm() {
    check_all("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn top_level_chain_hint_cranelift() {
    check_all("--jit");
}
