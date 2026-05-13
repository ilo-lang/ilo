// Regression test: a program whose entry function returns `Value::Ok(inner)`
// from the plain `~` arm must print just `inner` to stdout — no leading `~`
// wrapper — and exit 0.
//
// Background:
//
// PR #255 split `^e` (Err) output: stderr + exit 1, so shell pipelines can
// detect program failure without inspecting stdout. The companion `~v` (Ok)
// path was left on `Display`, which prints `~v`. Bash callers piping a
// Result-returning ilo program had to strip a leading `~` to consume the
// value:
//
//     path=$(ilo 'm>R t t;wrl "tasks.txt" "..."' | sed 's/^~//')
//
// This test pins the symmetric fix: `Value::Ok(v)` at the top-level program
// return prints `v` (via Display on the inner value) to stdout, exit 0.
// JSON mode still wraps as `{"ok": v}` — that contract is unchanged because
// machine consumers parse the envelope explicitly.
//
// `Display` on `Value::Ok` elsewhere (nested values, `prnt`, REPL, error
// messages, debug formatting) still renders `~v` — only the top-level
// program-return print path is split.
//
// All tests cross-engine (tree, VM, Cranelift) so a divergence between
// backends shows up in CI.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

// `main` that returns `Value::Ok(Value::Number(7))`. Signature `>R n t` =
// Result<n, t>, body returns `~7`.
const OK_NUM_SRC: &str = "m>R n t;~7";
// Returns `Value::Ok(Value::Text("tasks.txt"))` — the `wrl`-shaped use case
// the original assessment-doc entry flagged.
const OK_TEXT_SRC: &str = "m>R t t;~\"tasks.txt\"";
// Plain non-Result return — must still print via Display, no change.
const PLAIN_SRC: &str = "m>n;42";

// ── Plain mode: Value::Ok prints bare inner to stdout, exit 0 ──────────────

fn assert_ok_bare_plain(engine: &str, src: &str, expected_stdout: &str) {
    let out = ilo()
        .args([src, engine])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "{engine}: expected exit 0 for Value::Ok from main, got {:?}. stdout={:?} stderr={:?}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stdout_trim = stdout.trim();
    assert_eq!(
        stdout_trim, expected_stdout,
        "{engine}: expected bare {expected_stdout:?} on stdout (no `~` prefix), got {stdout_trim:?}",
    );
    assert!(
        !stdout_trim.starts_with('~'),
        "{engine}: stdout still leaks `~` wrapper: {stdout_trim:?}",
    );
}

#[test]
fn main_ok_num_bare_tree() {
    assert_ok_bare_plain("--run-tree", OK_NUM_SRC, "7");
}

#[test]
fn main_ok_num_bare_vm() {
    assert_ok_bare_plain("--run-vm", OK_NUM_SRC, "7");
}

#[test]
#[cfg(feature = "cranelift")]
fn main_ok_num_bare_cranelift() {
    assert_ok_bare_plain("--run-cranelift", OK_NUM_SRC, "7");
}

#[test]
fn main_ok_text_bare_tree() {
    assert_ok_bare_plain("--run-tree", OK_TEXT_SRC, "tasks.txt");
}

#[test]
fn main_ok_text_bare_vm() {
    assert_ok_bare_plain("--run-vm", OK_TEXT_SRC, "tasks.txt");
}

#[test]
#[cfg(feature = "cranelift")]
fn main_ok_text_bare_cranelift() {
    assert_ok_bare_plain("--run-cranelift", OK_TEXT_SRC, "tasks.txt");
}

// ── Plain mode: non-Result return unchanged ────────────────────────────────

fn assert_plain_unchanged(engine: &str) {
    let out = ilo()
        .args([PLAIN_SRC, engine])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "{engine}: expected exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        stdout.trim(),
        "42",
        "{engine}: plain non-Result return should print via Display unchanged",
    );
}

#[test]
fn main_plain_unchanged_tree() {
    assert_plain_unchanged("--run-tree");
}

#[test]
fn main_plain_unchanged_vm() {
    assert_plain_unchanged("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn main_plain_unchanged_cranelift() {
    assert_plain_unchanged("--run-cranelift");
}

// ── JSON mode: Value::Ok still wraps as {"ok": v} on stdout ────────────────
//
// The bare-stdout fix is plain-mode only. Machine consumers asking for
// `--json` explicitly want the envelope so they can discriminate ok vs
// error on a stable top-level key.

fn assert_ok_json_envelope(engine: &str) {
    let out = ilo()
        .args([OK_NUM_SRC, engine, "--json"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "{engine} -j: expected exit 0, got {:?}",
        out.status.code(),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("{engine} -j: stdout was not valid JSON ({e}): {stdout:?}"));
    let ok = parsed.get("ok").unwrap_or_else(|| {
        panic!("{engine} -j: expected `ok` key in {parsed:?}");
    });
    assert_eq!(
        ok,
        &serde_json::json!(7),
        "{engine} -j: ok payload mismatch"
    );
}

#[test]
fn main_ok_json_envelope_tree() {
    assert_ok_json_envelope("--run-tree");
}

#[test]
fn main_ok_json_envelope_vm() {
    assert_ok_json_envelope("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn main_ok_json_envelope_cranelift() {
    assert_ok_json_envelope("--run-cranelift");
}

// ── Display contract elsewhere is preserved: `prnt ~"x"` still shows `~x` ──
//
// Pinning this so a future "strip everywhere" refactor doesn't silently
// land. The parked entry in ilo_assessment_feedback.md tracks the
// separately-scoped semantic change for `prnt`; here we just guard that
// only the top-level print path was changed.

fn assert_prnt_wrapper_preserved(engine: &str) {
    // `prnt ~"x"` returns `~"x"` and prints it via Display; the program's
    // top-level return is the passthrough `~"x"`, which under the new
    // contract prints bare `x` on a second line. So stdout has two lines:
    //   ~x        <- from prnt (Display, wrapper visible)
    //   x         <- from top-level print_value (bare)
    let out = ilo()
        .args(["m>R t t;prnt ~\"x\"", engine])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "{engine}: expected exit 0, got {:?}. stderr={:?}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "{engine}: expected 2 stdout lines (prnt + top-level), got {stdout:?}",
    );
    assert_eq!(
        lines[0], "~x",
        "{engine}: prnt must preserve the `~` wrapper (Display), got {:?}",
        lines[0],
    );
    assert_eq!(
        lines[1], "x",
        "{engine}: top-level print must strip the `~` wrapper, got {:?}",
        lines[1],
    );
}

#[test]
fn prnt_wrapper_preserved_tree() {
    assert_prnt_wrapper_preserved("--run-tree");
}

#[test]
fn prnt_wrapper_preserved_vm() {
    assert_prnt_wrapper_preserved("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn prnt_wrapper_preserved_cranelift() {
    assert_prnt_wrapper_preserved("--run-cranelift");
}
