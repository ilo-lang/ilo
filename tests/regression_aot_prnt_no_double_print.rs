// Regression: `ilo compile` AOT binaries must NOT double-print when the
// entry function's body ends with a `prnt` call.
//
// Background: `prnt v` prints v on stdout and returns v (passthrough). The
// runtime auto-echoes the entry function's return value, so a body like
//
//     main>_;prnt "hello"
//
// used to produce `hello\nhello\n` — the in-builtin print, then a second
// copy from the top-level auto-echo. Caught by the `subscription-renewer`
// and `cron-explainer` personas (pending.md P0 #3).
//
// Fix touches two places:
//   1. `src/main.rs`: `program_result_should_suppress` now also returns
//      true when the last statement is a `prnt` call — covers tree/VM/JIT
//      which all route results through `print_value`.
//   2. `src/vm/compile_cranelift.rs` + `src/vm/mod.rs`: `generate_main`
//      consults `entry_should_suppress_auto_echo` and routes the result
//      through `jit_prt_main_result_suppress` (which suppresses the
//      happy-path stdout but still surfaces `^e` on stderr with exit 1)
//      when applicable — covers AOT binaries which don't go through
//      main.rs's print path.
//
// This file pins the AOT case. The in-process engine cases are covered by
// the unit tests in `src/main.rs` (`suppress_prnt_at_tail` and friends)
// and by `regression_loop_print.rs`. Gated on `cranelift` because AOT
// compile requires it.

#![cfg(feature = "cranelift")]

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn tmp_paths(tag: &str) -> (PathBuf, PathBuf) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let src = std::env::temp_dir().join(format!("ilo-aot-prnt-{tag}-{pid}-{n}.ilo"));
    let bin = std::env::temp_dir().join(format!("ilo-aot-prnt-{tag}-{pid}-{n}.bin"));
    (src, bin)
}

fn compile_and_run(src_body: &str, tag: &str) -> (String, String, i32) {
    let (src, bin) = tmp_paths(tag);
    std::fs::write(&src, src_body).expect("write src");

    let compile = ilo()
        .args(["compile"])
        .arg(&src)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("compile");
    assert!(
        compile.status.success(),
        "compile failed for src=`{src_body}`: stderr={:?}",
        String::from_utf8_lossy(&compile.stderr),
    );

    let run = Command::new(&bin).output().expect("run binary");
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();
    let code = run.status.code().unwrap_or(-1);

    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&bin);

    (stdout, stderr, code)
}

// ── 1. Bare `prnt` at tail: AOT binary prints exactly once ────────────────

#[test]
fn aot_prnt_at_tail_does_not_double_print() {
    let (stdout, _stderr, code) = compile_and_run("main>_;prnt \"hello\"\n", "prnt-tail-text");

    // Pre-fix behaviour: stdout was "hello\nhello\n" — the prnt builtin
    // wrote "hello", then `jit_prt_main_result` wrote it again. Post-fix:
    // exactly one "hello\n".
    assert_eq!(
        stdout, "hello\n",
        "AOT must print `hello` exactly once, got: {stdout:?}"
    );
    assert_eq!(code, 0, "wrong exit: {code}");
}

// ── 2. `prnt` of a number at tail: also single-print ──────────────────────

#[test]
fn aot_prnt_number_at_tail_does_not_double_print() {
    let (stdout, _stderr, code) = compile_and_run("main>_;prnt 42\n", "prnt-tail-num");

    assert_eq!(
        stdout, "42\n",
        "AOT must print `42` exactly once, got: {stdout:?}"
    );
    assert_eq!(code, 0, "wrong exit: {code}");
}

// ── 3. `prnt` NOT at tail: final value still auto-echoes ──────────────────

#[test]
fn aot_prnt_not_at_tail_still_echoes_final_value() {
    // Last stmt is the string `"done"`, not a `prnt` call. The runtime
    // auto-echo must still fire so the program return value reaches stdout.
    // Total output: "hi\ndone\n" — prnt writes "hi", auto-echo writes "done".
    let (stdout, _stderr, code) = compile_and_run("main>_;prnt \"hi\";\"done\"\n", "prnt-not-tail");

    assert_eq!(
        stdout, "hi\ndone\n",
        "non-tail prnt must keep printing and tail expr must echo, got: {stdout:?}"
    );
    assert_eq!(code, 0, "wrong exit: {code}");
}

// ── 4. Plain string at tail: AOT still auto-echoes (no suppression) ───────

#[test]
fn aot_plain_expr_at_tail_still_echoes() {
    // No prnt anywhere — runtime auto-echo is the only source of stdout.
    // Suppression must NOT fire here, or the program produces no output.
    let (stdout, _stderr, code) = compile_and_run("main>_;\"hello\"\n", "plain-tail");

    assert_eq!(
        stdout, "hello\n",
        "plain tail expr must still auto-echo, got: {stdout:?}"
    );
    assert_eq!(code, 0, "wrong exit: {code}");
}

// ── 5. Loop with prnt at tail: existing loop-tail suppression unchanged ──

#[test]
fn aot_loop_with_prnt_body_does_not_double_print() {
    // Pre-existing rule (PR for loop-tail suppression): a loop body's prnt
    // already wrote stdout, the loop-tail value would duplicate the last
    // item. This test pins that AOT respects that rule too — the suppress
    // helper is now used uniformly for both prnt-at-tail and loop-at-tail.
    let (stdout, _stderr, code) =
        compile_and_run("main>_;@x [\"a\" \"b\" \"c\"]{prnt x}\n", "loop-prnt");

    assert_eq!(
        stdout, "a\nb\nc\n",
        "loop body's prnts must not be duplicated, got: {stdout:?}"
    );
    assert_eq!(code, 0, "wrong exit: {code}");
}
