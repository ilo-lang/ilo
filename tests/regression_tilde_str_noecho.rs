// Regression test: entry-fn bodies that `prnt` their real output and return
// `~"ok"` (or any bare `~"text"`/`^"text"` literal) as a status sentinel must
// NOT emit a trailing auto-echo line on stdout. The auto-echo collides with
// the program's intended output and forces shell callers to strip a `ok`
// suffix.
//
// Surfaced by the `log-timeline-merger` persona (logs.md 2026-05-20):
// > `prnt` on a wrapped result prints both the prnt output and the return
// > value. `main` returning `~"ok"` prints `ok` after all `prnt` lines.
// > Fine for CLI scripts, but if the caller captures stdout for further
// > processing, they get a trailing `ok` to strip.
//
// The suppression rule (in `program_result_should_suppress`):
//
//   * The entry function has at least one *unconditional top-level* `prnt`
//     call (not nested inside a guard/loop/match — those are conditional).
//   * The tail expression is `~"<literal>"` or `^"<literal>"`.
//
// Anything outside that shape keeps its existing auto-echo behaviour. In
// particular:
//
//   * `prnt` inside a guard body whose tail is `~"done"` (see
//     `cond-multi-stmt-guard-return.ilo`) still auto-echoes — the guard
//     might not have fired, so the `prnt` may not have written.
//   * Functions with no `prnt` at all (e.g. `addtask` in
//     `cli-tasks-save-ok.ilo` returning `~"ok"`) still print `ok` —
//     the `~"ok"` is the only thing they emit, and `cli-tasks-save-ok`
//     is the canonical example of that pattern.
//   * Bare `~v` where `v` is a non-literal (binding, call) still
//     auto-echoes — it's a real return value the caller asked for.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

// ── Core fix: prnt + ~"text" tail suppresses the auto-echo ────────────────

fn assert_no_trailing_status(engine: &str, src: &str, expected_stdout: &str) {
    let out = ilo()
        .args([src, engine])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "{engine}: expected exit 0, got {:?}. stdout={:?} stderr={:?}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        stdout.trim_end_matches('\n'),
        expected_stdout,
        "{engine}: stdout should not include the wrapped-literal status sentinel",
    );
}

// `m>R t t;prnt "first";prnt "second";~"ok"`. Pre-fix: stdout was
// "first\nsecond\nok\n". Post-fix: "first\nsecond\n".
const PRNT_PLUS_TILDE_OK: &str = "m>R t t;prnt \"first\";prnt \"second\";~\"ok\"";

#[test]
fn prnt_plus_tilde_ok_no_trailing_tree() {
    assert_no_trailing_status("--vm", PRNT_PLUS_TILDE_OK, "first\nsecond");
}

#[test]
fn prnt_plus_tilde_ok_no_trailing_vm() {
    assert_no_trailing_status("--vm", PRNT_PLUS_TILDE_OK, "first\nsecond");
}

#[test]
#[cfg(feature = "cranelift")]
fn prnt_plus_tilde_ok_no_trailing_cranelift() {
    assert_no_trailing_status("--jit", PRNT_PLUS_TILDE_OK, "first\nsecond");
}

// ── Non-string Ok (`~7`) still auto-echoes ────────────────────────────────
//
// The suppression only fires for a bare *string literal* under `~`/`^`.
// A numeric or binding-valued Ok is a real return value the caller wants
// captured. Pin the contract so a future change to "suppress all Ok-literal
// returns when there's a prnt" doesn't accidentally swallow real numbers.

const PRNT_PLUS_TILDE_NUM: &str = "m>R n t;prnt \"x\";~42";

fn assert_auto_echo_present(engine: &str, src: &str, expected_last_line: &str) {
    let out = ilo()
        .args([src, engine])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "{engine}: expected exit 0");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let last = stdout.lines().last().unwrap_or("");
    assert_eq!(
        last, expected_last_line,
        "{engine}: numeric Ok must still auto-echo, got stdout={stdout:?}",
    );
}

#[test]
fn prnt_plus_tilde_num_still_echoes_tree() {
    assert_auto_echo_present("--vm", PRNT_PLUS_TILDE_NUM, "42");
}

#[test]
fn prnt_plus_tilde_num_still_echoes_vm() {
    assert_auto_echo_present("--vm", PRNT_PLUS_TILDE_NUM, "42");
}

#[test]
#[cfg(feature = "cranelift")]
fn prnt_plus_tilde_num_still_echoes_cranelift() {
    assert_auto_echo_present("--jit", PRNT_PLUS_TILDE_NUM, "42");
}

// ── No-prnt body returning `~"ok"` still echoes ───────────────────────────
//
// This is the `cli-tasks-save-ok.ilo` pattern. `addtask` does no `prnt`;
// its only output is the `~"ok"` it returns. The auto-echo of `ok` is the
// program's intended output and must not be suppressed.

const NO_PRNT_TILDE_OK: &str = "m>R t t;~\"ok\"";

#[test]
fn no_prnt_tilde_ok_still_echoes_tree() {
    assert_auto_echo_present("--vm", NO_PRNT_TILDE_OK, "ok");
}

#[test]
fn no_prnt_tilde_ok_still_echoes_vm() {
    assert_auto_echo_present("--vm", NO_PRNT_TILDE_OK, "ok");
}

#[test]
#[cfg(feature = "cranelift")]
fn no_prnt_tilde_ok_still_echoes_cranelift() {
    assert_auto_echo_present("--jit", NO_PRNT_TILDE_OK, "ok");
}

// ── `prnt` only inside a non-firing guard body — auto-echo still fires ────
//
// Pins the `cond-multi-stmt-guard-return.ilo` contract. The `prnt` here is
// inside `=n 0{...}` and doesn't run when n != 0. We must NOT suppress
// based on a `prnt` that may never execute — the static "body contains
// prnt" check would be wrong. Only top-level unconditional `prnt`s count.

const GUARD_PRNT_TILDE_DONE: &str = "m n:n>R t t;=n 0{prnt \"empty\";~\"ok\"};~\"done\"";

fn assert_arged_auto_echo(engine: &str, src: &str, arg: &str, expected: &str) {
    let out = ilo()
        .args([src, engine, "m", arg])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "{engine}: expected exit 0, got {:?}. stderr={:?}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr),
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        stdout.trim(),
        expected,
        "{engine}: guard-only prnt body must still auto-echo the tail value",
    );
}

#[test]
fn guard_only_prnt_still_echoes_done_tree() {
    assert_arged_auto_echo("--vm", GUARD_PRNT_TILDE_DONE, "1", "done");
}

#[test]
fn guard_only_prnt_still_echoes_done_vm() {
    assert_arged_auto_echo("--vm", GUARD_PRNT_TILDE_DONE, "1", "done");
}

#[test]
#[cfg(feature = "cranelift")]
fn guard_only_prnt_still_echoes_done_cranelift() {
    assert_arged_auto_echo("--jit", GUARD_PRNT_TILDE_DONE, "1", "done");
}

// ── `^"text"` mirror: Err always goes to stderr, so suppression is benign ─
//
// We pin that the Err path keeps writing to stderr (and exits 1) even when
// the body has a `prnt`. The suppress flag never silently swallows Err.

const PRNT_PLUS_CARET_BAD: &str = "m>R t t;prnt \"diagnostic\";^\"bad\"";

#[test]
fn prnt_plus_caret_err_still_on_stderr_vm() {
    let out = ilo()
        .args([PRNT_PLUS_CARET_BAD, "--vm"])
        .output()
        .expect("failed to run ilo");
    assert_eq!(
        out.status.code(),
        Some(1),
        "Err return must exit 1 even with prnt-suppression rule active",
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("bad"),
        "Err payload must still surface on stderr, got {stderr:?}",
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        stdout.trim(),
        "diagnostic",
        "Stdout should hold the prnt output only, got {stdout:?}",
    );
}
