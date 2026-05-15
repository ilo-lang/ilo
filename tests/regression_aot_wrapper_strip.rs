// Regression test: AOT-compiled binaries must strip the top-level `~`/`^`
// Result wrapper on stdout/stderr the same way the in-process runners do.
//
// Background:
//
// PR #275 split top-level Result handling for the in-process runners
// (tree, VM, Cranelift JIT) so that `~v` returned from main prints `v`
// bare on stdout (exit 0) and `^e` prints `^e` on stderr (exit 1). The
// AOT path (`ilo compile main.ilo -o ./main && ./main`) was left calling
// the `jit_prt` helper directly from `generate_main`, so AOT binaries
// kept printing the visible wrapper and always exited 0 — even for `^e`.
//
// The fix routes `generate_main`'s final result through a new helper,
// `jit_prt_main_result`, that mirrors `print_value`'s top-level treatment
// and returns the desired process exit code. The in-program `prnt`
// builtin still uses `jit_prt` (which always shows the wrapper) — that
// path is *inside* a program and genuinely wants `~v` / `^e` visible.
//
// Each case here compares the AOT binary's stdout, stderr, and exit code
// against the three in-process runners byte-for-byte, so any future
// divergence between AOT and the others shows up immediately in CI.
//
// Gated on the `cranelift` feature because both AOT compile and the
// `--run-cranelift` baseline require it.

#![cfg(feature = "cranelift")]

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Per-process counter so concurrent test threads don't stomp each
/// other's source / binary paths in /tmp.
static COUNTER: AtomicU32 = AtomicU32::new(0);

fn tmp_paths(tag: &str) -> (PathBuf, PathBuf) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let src = std::env::temp_dir().join(format!("ilo-aot-{tag}-{pid}-{n}.ilo"));
    let bin = std::env::temp_dir().join(format!("ilo-aot-{tag}-{pid}-{n}.bin"));
    (src, bin)
}

/// Run the in-process Cranelift runner and capture (stdout, stderr, exit).
fn run_in_process(src_path: &PathBuf, engine: &str) -> (Vec<u8>, Vec<u8>, i32) {
    let out = ilo()
        .arg(src_path)
        .arg(engine)
        .output()
        .expect("failed to run ilo in-process");
    (out.stdout, out.stderr, out.status.code().unwrap_or(-1))
}

/// Compile the source to an AOT binary, run it, capture (stdout, stderr, exit).
fn run_aot(src_path: &PathBuf, bin_path: &PathBuf) -> (Vec<u8>, Vec<u8>, i32) {
    let compile = ilo()
        .args(["compile"])
        .arg(src_path)
        .arg("-o")
        .arg(bin_path)
        .output()
        .expect("failed to invoke ilo compile");
    assert!(
        compile.status.success(),
        "ilo compile failed: stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr),
    );
    let out = Command::new(bin_path)
        .output()
        .expect("failed to run AOT binary");
    (out.stdout, out.stderr, out.status.code().unwrap_or(-1))
}

/// Assert AOT output matches all three in-process runners (tree, VM,
/// Cranelift) byte-for-byte, and matches the explicit expected values.
fn assert_aot_matches_in_process(
    tag: &str,
    src: &str,
    expected_stdout: &[u8],
    expected_stderr: &[u8],
    expected_exit: i32,
) {
    let (src_path, bin_path) = tmp_paths(tag);
    std::fs::write(&src_path, src).expect("write ilo source");

    let (aot_stdout, aot_stderr, aot_exit) = run_aot(&src_path, &bin_path);

    assert_eq!(
        aot_stdout,
        expected_stdout,
        "{tag}: AOT stdout mismatch. got={:?} expected={:?}",
        String::from_utf8_lossy(&aot_stdout),
        String::from_utf8_lossy(expected_stdout),
    );
    assert_eq!(
        aot_stderr,
        expected_stderr,
        "{tag}: AOT stderr mismatch. got={:?} expected={:?}",
        String::from_utf8_lossy(&aot_stderr),
        String::from_utf8_lossy(expected_stderr),
    );
    assert_eq!(
        aot_exit, expected_exit,
        "{tag}: AOT exit mismatch. got={aot_exit} expected={expected_exit}",
    );

    // Cross-engine parity: AOT must match all three in-process runners
    // byte-for-byte. This is the contract PR #275 set for in-process and
    // this PR extends to AOT.
    for engine in ["--run-tree", "--run-vm", "--run-cranelift"] {
        let (s, e, c) = run_in_process(&src_path, engine);
        assert_eq!(
            s,
            aot_stdout,
            "{tag}/{engine}: stdout diverges from AOT. in-proc={:?} aot={:?}",
            String::from_utf8_lossy(&s),
            String::from_utf8_lossy(&aot_stdout),
        );
        assert_eq!(
            e,
            aot_stderr,
            "{tag}/{engine}: stderr diverges from AOT. in-proc={:?} aot={:?}",
            String::from_utf8_lossy(&e),
            String::from_utf8_lossy(&aot_stderr),
        );
        assert_eq!(
            c, aot_exit,
            "{tag}/{engine}: exit diverges from AOT. in-proc={c} aot={aot_exit}",
        );
    }

    // Best-effort cleanup; ignore failures.
    let _ = std::fs::remove_file(&src_path);
    let _ = std::fs::remove_file(&bin_path);
}

// ── ~"hello" → stdout `hello\n`, exit 0 ───────────────────────────────────

#[test]
fn aot_ok_text_strips_wrapper_to_stdout() {
    assert_aot_matches_in_process("ok-text", "m>R t t;~\"hello\"", b"hello\n", b"", 0);
}

// ── ^"err" → stderr `^err\n`, exit 1 ──────────────────────────────────────

#[test]
fn aot_err_routes_to_stderr_exit_1() {
    assert_aot_matches_in_process("err-text", "m>R t t;^\"err\"", b"", b"^err\n", 1);
}

// ── bare 42 → stdout `42\n`, exit 0 ───────────────────────────────────────

#[test]
fn aot_bare_value_unchanged() {
    assert_aot_matches_in_process("bare-num", "m>n;42", b"42\n", b"", 0);
}

// ── ~7 (number-typed Result) → stdout `7\n`, exit 0 ───────────────────────
//
// Pins the wrapper-strip for a number-typed Result variant in addition to
// the text variant above. A regression that strips only `~"text"` and not
// `~num` would otherwise slip through.

#[test]
fn aot_ok_num_strips_wrapper_to_stdout() {
    assert_aot_matches_in_process("ok-num", "m>R n t;~7", b"7\n", b"", 0);
}
