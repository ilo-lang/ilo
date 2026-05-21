// Regression tests for `fmt` printf-style placeholder specs (#16c).
//
// `fmt` supports a small, deliberate subset of printf-style specs so agents
// don't have to compose `fmt2` + `padl` for the common cases:
//
//   {}         — bare placeholder, Display
//   {.Nf}      — N decimal places (no colon)
//   {:.Nf}     — N decimal places (with colon)
//   {:N}       — right-align Display to width N (space-pad)
//   {:Nd}      — integer right-align to width N (truncate toward zero)
//   {:<N}      — left-align Display to width N
//
// Zero-padded widths (`{:06d}`), sign-prefix (`{:+}`), hex (`{:x}`) and
// other shapes are deliberately out of scope and must surface as errors:
//   - Verify-time (ILO-T013) when the template is a literal,
//   - Runtime (ILO-R009) when the template is computed.
//
// Each spec is exercised on tree, VM, and Cranelift to keep all three
// engines in lockstep (the interpreter is the single source of truth via
// the tree-bridge, but the cross-engine guard catches future drift).

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

// ── 1) Bare `{}` still works on every engine ───────────────────────────────

const BARE_OK: &str = "f>t;fmt \"x={}\" 42";

fn check_bare_ok(engine: &str) {
    assert_eq!(run_ok(engine, BARE_OK, "f"), "x=42", "engine={engine}");
}

#[test]
fn bare_ok_tree() {
    check_bare_ok("--vm");
}

#[test]
fn bare_ok_vm() {
    check_bare_ok("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn bare_ok_cranelift() {
    check_bare_ok("--jit");
}

// ── 2) `{.Nf}` precision shorthand ────────────────────────────────────────

const PREC_SHORT: &str = "f>t;fmt \"{.2f}\" 3.14159";

fn check_prec_short(engine: &str) {
    assert_eq!(run_ok(engine, PREC_SHORT, "f"), "3.14", "engine={engine}");
}

#[test]
fn prec_short_tree() {
    check_prec_short("--vm");
}

#[test]
fn prec_short_vm() {
    check_prec_short("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn prec_short_cranelift() {
    check_prec_short("--jit");
}

// ── 3) `{:.Nf}` precision long-form ───────────────────────────────────────

const PREC_LONG: &str = "f>t;fmt \"pi={:.3f}\" 3.14159";

fn check_prec_long(engine: &str) {
    assert_eq!(
        run_ok(engine, PREC_LONG, "f"),
        "pi=3.142",
        "engine={engine}"
    );
}

#[test]
fn prec_long_tree() {
    check_prec_long("--vm");
}

#[test]
fn prec_long_vm() {
    check_prec_long("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn prec_long_cranelift() {
    check_prec_long("--jit");
}

// ── 4) `{:N}` right-align width for strings ───────────────────────────────

const WIDTH_RIGHT_STR: &str = "f>t;fmt \"[{:5}]\" \"hi\"";

fn check_width_right_str(engine: &str) {
    assert_eq!(
        run_ok(engine, WIDTH_RIGHT_STR, "f"),
        "[   hi]",
        "engine={engine}"
    );
}

#[test]
fn width_right_str_tree() {
    check_width_right_str("--vm");
}

#[test]
fn width_right_str_vm() {
    check_width_right_str("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn width_right_str_cranelift() {
    check_width_right_str("--jit");
}

// ── 5) `{:Nd}` integer width ──────────────────────────────────────────────

const INT_WIDTH: &str = "f>t;fmt \"[{:5d}]\" 42";

fn check_int_width(engine: &str) {
    assert_eq!(run_ok(engine, INT_WIDTH, "f"), "[   42]", "engine={engine}");
}

#[test]
fn int_width_tree() {
    check_int_width("--vm");
}

#[test]
fn int_width_vm() {
    check_int_width("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn int_width_cranelift() {
    check_int_width("--jit");
}

// ── 6) `{:<N}` left-align width ───────────────────────────────────────────

const WIDTH_LEFT: &str = "f>t;fmt \"[{:<5}]\" \"hi\"";

fn check_width_left(engine: &str) {
    assert_eq!(
        run_ok(engine, WIDTH_LEFT, "f"),
        "[hi   ]",
        "engine={engine}"
    );
}

#[test]
fn width_left_tree() {
    check_width_left("--vm");
}

#[test]
fn width_left_vm() {
    check_width_left("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn width_left_cranelift() {
    check_width_left("--jit");
}

// ── 7) Multiple mixed specs in one template ───────────────────────────────
//
// Cross-cuts the slot-counting path in the verifier: 3 placeholders need 3
// value args, with each spec applied to its own arg.

const MIXED: &str = "f>t;fmt \"{} {:.2f} [{:<3}]\" 1 3.14159 \"x\"";

fn check_mixed(engine: &str) {
    assert_eq!(
        run_ok(engine, MIXED, "f"),
        "1 3.14 [x  ]",
        "engine={engine}"
    );
}

#[test]
fn mixed_tree() {
    check_mixed("--vm");
}

#[test]
fn mixed_vm() {
    check_mixed("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn mixed_cranelift() {
    check_mixed("--jit");
}

// ── 8) Verify-time rejection of zero-padded width `{:06d}` ────────────────
//
// Out of scope — we promise we won't grow the spec without a deliberate
// design pass, and the rejection error must point agents at the supported
// composition pattern (`padl`).

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
        s.contains("padl") || s.contains("fmt2"),
        "engine={engine}: expected composition hint, got: {s}"
    );
}

#[test]
fn literal_06d_tree() {
    check_literal_06d("--vm");
}

#[test]
fn literal_06d_vm() {
    check_literal_06d("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn literal_06d_cranelift() {
    check_literal_06d("--jit");
}

// ── 9) Verify-time rejection of `{:.N}` without the `f` ───────────────────
//
// Pre-fix this was "all `{:` is bad"; post-fix we accept `{:.3f}` but still
// reject `{:.3}` because there's no obvious default conversion (int? float?
// string?) and silent ambiguity is exactly the trap the spec is here to
// avoid.

const LITERAL_PREC_NO_F: &str = "f>t;fmt \"{:.3}\" 3.14159";

fn check_literal_prec_no_f(engine: &str) {
    let s = run_err(engine, LITERAL_PREC_NO_F, "f");
    assert!(
        s.contains("ILO-T013") && s.contains("{:.3}"),
        "engine={engine}: expected ILO-T013 mentioning {{:.3}}, got: {s}"
    );
}

#[test]
fn literal_prec_no_f_tree() {
    check_literal_prec_no_f("--vm");
}

#[test]
fn literal_prec_no_f_vm() {
    check_literal_prec_no_f("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn literal_prec_no_f_cranelift() {
    check_literal_prec_no_f("--jit");
}

// ── 10) Runtime rejection when the template is computed ───────────────────
//
// Verifier can't see inside `cat`/variables, so the bad spec only surfaces
// at runtime. The interpreter must still reject it (ILO-R009).

const COMPUTED_06D: &str = "f>t;t=cat [\"{:06d}\"] \"\";fmt t 42";

fn check_computed_06d(engine: &str) {
    let s = run_err(engine, COMPUTED_06D, "f");
    assert!(
        s.contains("ILO-R009"),
        "engine={engine}: expected ILO-R009 from runtime fmt, got: {s}"
    );
    assert!(
        s.contains("{:06d}"),
        "engine={engine}: expected offending spec in message, got: {s}"
    );
}

#[test]
fn computed_06d_tree() {
    check_computed_06d("--vm");
}

#[test]
fn computed_06d_vm() {
    check_computed_06d("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn computed_06d_cranelift() {
    check_computed_06d("--jit");
}

// ── 11) A lone `{` followed by non-spec text still passes through ─────────
//
// `fmt "{a:1}"` is JSON-ish text, not a placeholder — the leading `{` is
// followed by an identifier, not `}` / `:` / `.`. Must round-trip
// unchanged.

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
    check_lone_brace_ok("--vm");
}

#[test]
fn lone_brace_ok_vm() {
    check_lone_brace_ok("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn lone_brace_ok_cranelift() {
    check_lone_brace_ok("--jit");
}

// ── 12) Width shorter than content leaves content untouched ───────────────
//
// Padding only adds space when the rendered length is shorter than N; it
// never truncates. This matches `padl`/`padr` semantics.

const WIDTH_NO_PAD: &str = "f>t;fmt \"[{:3}]\" \"hello\"";

fn check_width_no_pad(engine: &str) {
    assert_eq!(
        run_ok(engine, WIDTH_NO_PAD, "f"),
        "[hello]",
        "engine={engine}"
    );
}

#[test]
fn width_no_pad_tree() {
    check_width_no_pad("--vm");
}

#[test]
fn width_no_pad_vm() {
    check_width_no_pad("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn width_no_pad_cranelift() {
    check_width_no_pad("--jit");
}
