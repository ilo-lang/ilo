// Regression tests for `fmt` rejecting printf-style `{:...}` format specs.
//
// Before this fix, `fmt "{:06d}" 42` silently returned the literal string
// `"{:06d}"` because the placeholder scanner only matched the exact pair
// `{}` and let everything else through. Personas reaching for Python-style
// padding/precision (e.g. for sort-key construction) got silent wrong
// output and minutes of "where did my data go" debugging. See the
// pdf-analyst rerun3 entry in ilo_assessment_feedback.md.
//
// Fix: bare `{}` placeholders only. `{:...}` is now a hard error.
//   - Verify time (ILO-T013): when the template is a string literal we
//     reject it before the program ever runs.
//   - Runtime (ILO-R009): when the template is computed at runtime we
//     still catch it inside the `fmt` interpreter (single source of truth
//     for tree / VM / Cranelift via the tree-bridge).
//
// The error message points at idiomatic substitutes (`fmt2` for decimal
// precision, `padl` for padding).

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(name: &str, src: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("ilo_fmtspec_{name}_{}_{n}.ilo", std::process::id()));
    std::fs::write(&path, src).expect("write src");
    path
}

fn run_ok(engine: &str, src: &str, entry: &str) -> String {
    let path = write_src(entry, src);
    let out = ilo()
        .arg(&path)
        .arg(engine)
        .arg(entry)
        .output()
        .expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(engine: &str, src: &str, entry: &str) -> String {
    let path = write_src(entry, src);
    let out = ilo()
        .arg(&path)
        .arg(engine)
        .arg(entry)
        .output()
        .expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        !out.status.success(),
        "expected failure but ilo {engine} succeeded for `{src}`"
    );
    let mut s = String::from_utf8_lossy(&out.stderr).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stdout));
    s
}

// ── 1) Verify-time rejection for literal templates ─────────────────────────
//
// The template is a string literal so the verifier catches the bad spec
// before any engine runs. Same error on every engine.

const LITERAL_06D: &str = "f>t;fmt \"{:06d}\" 42";

fn check_literal_06d(engine: &str) {
    let s = run_err(engine, LITERAL_06D, "f");
    assert!(
        s.contains("ILO-T013"),
        "engine={engine}: expected ILO-T013, got: {s}"
    );
    assert!(
        s.contains("{:06d}"),
        "engine={engine}: expected offending spec in message, got: {s}"
    );
    assert!(
        s.contains("fmt2") && s.contains("padl"),
        "engine={engine}: expected fmt2+padl hint, got: {s}"
    );
}

#[test]
fn literal_06d_tree() {
    check_literal_06d("--run-tree");
}

#[test]
fn literal_06d_vm() {
    check_literal_06d("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn literal_06d_cranelift() {
    check_literal_06d("--run-cranelift");
}

// ── 2) Verify-time rejection for `{:.3f}` precision spec ───────────────────

const LITERAL_3F: &str = "f>t;fmt \"pi={:.3f}\" 3.14159";

fn check_literal_3f(engine: &str) {
    let s = run_err(engine, LITERAL_3F, "f");
    assert!(
        s.contains("ILO-T013") && s.contains("{:.3f}"),
        "engine={engine}: expected ILO-T013 mentioning {{:.3f}}, got: {s}"
    );
}

#[test]
fn literal_3f_tree() {
    check_literal_3f("--run-tree");
}

#[test]
fn literal_3f_vm() {
    check_literal_3f("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn literal_3f_cranelift() {
    check_literal_3f("--run-cranelift");
}

// ── 3) Runtime rejection when the template is computed ─────────────────────
//
// Verifier can't see inside `cat`/variables, so the bad spec only surfaces
// at runtime. The interpreter must still reject it (ILO-R009) rather than
// silently emit the literal `{:06d}` like the pre-fix behaviour.

const COMPUTED_06D: &str = "f>t;t=cat [\"x=\" \"{:06d}\"] \"\";fmt t 42";

fn check_computed_06d(engine: &str) {
    let s = run_err(engine, COMPUTED_06D, "f");
    assert!(
        s.contains("ILO-R009"),
        "engine={engine}: expected ILO-R009 from runtime fmt, got: {s}"
    );
    assert!(
        s.contains("{:06d}") && s.contains("fmt2"),
        "engine={engine}: expected offending spec + fmt2 hint, got: {s}"
    );
}

#[test]
fn computed_06d_tree() {
    check_computed_06d("--run-tree");
}

#[test]
fn computed_06d_vm() {
    check_computed_06d("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn computed_06d_cranelift() {
    check_computed_06d("--run-cranelift");
}

// ── 4) Bare `{}` still works on every engine ───────────────────────────────
//
// Regression guard: the new branch must not break the supported
// placeholder. `fmt "x={}" 42` → `"x=42"` on tree, VM, and Cranelift.

const BARE_OK: &str = "f>t;fmt \"x={}\" 42";

fn check_bare_ok(engine: &str) {
    assert_eq!(run_ok(engine, BARE_OK, "f"), "x=42", "engine={engine}");
}

#[test]
fn bare_ok_tree() {
    check_bare_ok("--run-tree");
}

#[test]
fn bare_ok_vm() {
    check_bare_ok("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn bare_ok_cranelift() {
    check_bare_ok("--run-cranelift");
}

// ── 5) A lone `{` followed by non-`:` non-`}` still passes through ────────
//
// Not every `{` is a placeholder — e.g. JSON-like text. Only `{:` and `{}`
// are reserved; everything else stays a literal.

const LONE_BRACE_OK: &str = "f>t;fmt \"{a:1}\"";

fn check_lone_brace_ok(engine: &str) {
    assert_eq!(
        run_ok(engine, LONE_BRACE_OK, "f"),
        "{a:1}",
        "engine={engine}"
    );
}

#[test]
fn lone_brace_ok_tree() {
    check_lone_brace_ok("--run-tree");
}

#[test]
fn lone_brace_ok_vm() {
    check_lone_brace_ok("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn lone_brace_ok_cranelift() {
    check_lone_brace_ok("--run-cranelift");
}
