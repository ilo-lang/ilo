// Regression test: AOT-compiled binaries must surface fatal signals as
// `ILO-R015` JSON diagnostics on stderr instead of dying silently with
// a raw signal exit code (e.g. 139 for SIGSEGV).
//
// Background:
//
// Tree, VM, and Cranelift JIT all route runtime errors through the
// `JIT_RUNTIME_ERROR` TLS channel and emit structured `ILO-R###`
// diagnostics. AOT-compiled binaries execute as standalone native
// code, so an uncaught fault (NULL deref, integer div-by-zero via
// SIGFPE, illegal instruction, abort()) was leaving the process with
// no JSON on stderr — agents driving ilo couldn't distinguish a hard
// fault from a clean non-zero exit.
//
// The fix installs an async-signal-safe handler in `ilo_aot_init`
// that writes a fixed JSON line for the signal then re-raises with
// SIG_DFL so the OS reports the conventional 128+signo exit.
//
// To exercise the handler deterministically without depending on a
// real codegen fault, `ilo_aot_init` honours an `ILO_FORCE_AOT_SIGNAL`
// env var (test-only path, mirrors `ILO_FORCE_JIT_PANIC` in the JIT).
// This test compiles a trivial AOT program and runs it with the var
// set to each supported signal, asserting the JSON shape on stderr
// and the conventional 128+signo exit code.
//
// Gated on the `cranelift` feature (AOT requires it) and `unix` (the
// handler uses sigaction; Windows is a no-op stub for now).

#![cfg(all(feature = "cranelift", unix))]

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
    let src = std::env::temp_dir().join(format!("ilo-aot-sig-{tag}-{pid}-{n}.@"));
    let bin = std::env::temp_dir().join(format!("ilo-aot-sig-{tag}-{pid}-{n}.bin"));
    (src, bin)
}

/// Compile a trivial AOT binary that we can run with the force-signal
/// env var. The program itself is irrelevant — the signal fires during
/// `ilo_aot_init` before user code executes.
fn compile_trivial(tag: &str) -> (PathBuf, PathBuf) {
    let (src, bin) = tmp_paths(tag);
    std::fs::write(&src, "m>n;1").expect("write ilo source");
    let out = ilo()
        .args(["compile"])
        .arg(&src)
        .arg("-o")
        .arg(&bin)
        .output()
        .expect("invoke ilo compile");
    assert!(
        out.status.success(),
        "ilo compile failed: stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    (src, bin)
}

/// Run the AOT binary with `ILO_FORCE_AOT_SIGNAL=<which>` and assert
/// the JSON-on-stderr + 128+signo exit code contract.
fn assert_signal_emits_json(which: &str, expected_signal_name: &str, expected_signo: i32) {
    let (src, bin) = compile_trivial(which);
    let out = Command::new(&bin)
        .env("ILO_FORCE_AOT_SIGNAL", which)
        .output()
        .expect("run AOT binary");

    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);

    // The JSON line must appear on stderr. Asserting on the substring
    // (not full equality) leaves room for future fields like a span or
    // backtrace hash without breaking this test.
    assert!(
        stderr.contains("\"code\":\"ILO-R015\""),
        "[{which}] stderr missing ILO-R015 code. stderr={stderr:?} stdout={stdout:?}",
    );
    assert!(
        stderr.contains(&format!("\"signal\":\"{expected_signal_name}\"")),
        "[{which}] stderr missing signal={expected_signal_name}. stderr={stderr:?}",
    );
    assert!(
        stderr.contains("\"short\":\"AOT runtime fault\""),
        "[{which}] stderr missing short. stderr={stderr:?}",
    );

    // Stderr must be parseable as JSON on its own line — no interleaved
    // log spam. Find the line and parse it.
    let json_line = stderr
        .lines()
        .find(|l| l.contains("ILO-R015"))
        .unwrap_or_else(|| panic!("[{which}] no ILO-R015 line on stderr: {stderr:?}"));
    // Minimal JSON parse: each field must round-trip as a key/value pair.
    // Avoids pulling serde into the test by checking shape with
    // substring assertions only.
    assert!(json_line.starts_with('{') && json_line.ends_with('}'));

    // Exit code is conventionally 128 + signo when the OS terminates a
    // process via signal. On some platforms `status.code()` returns
    // None and the signal shows up under `.signal()`; assert via raw
    // exit status code.
    use std::os::unix::process::ExitStatusExt;
    let signal = out.status.signal();
    let code = out.status.code();
    assert!(
        signal == Some(expected_signo) || code == Some(128 + expected_signo),
        "[{which}] expected signal={expected_signo} or code={}, got signal={signal:?} code={code:?}",
        128 + expected_signo,
    );

    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&bin);
}

#[test]
fn aot_sigsegv_emits_r015_json() {
    assert_signal_emits_json("segv", "SIGSEGV", libc::SIGSEGV);
}

#[test]
fn aot_sigabrt_emits_r015_json() {
    assert_signal_emits_json("abrt", "SIGABRT", libc::SIGABRT);
}

#[test]
fn aot_sigfpe_emits_r015_json() {
    assert_signal_emits_json("fpe", "SIGFPE", libc::SIGFPE);
}

#[test]
fn aot_sigill_emits_r015_json() {
    assert_signal_emits_json("ill", "SIGILL", libc::SIGILL);
}

#[test]
fn aot_sigbus_emits_r015_json() {
    assert_signal_emits_json("bus", "SIGBUS", libc::SIGBUS);
}

/// Without the env var, AOT binaries must run normally (no spurious
/// signal handler interference). This is the negative case that pins
/// "handler installed but inert under normal conditions".
#[test]
fn aot_no_signal_runs_normally() {
    let (src, bin) = compile_trivial("normal");
    let out = Command::new(&bin).output().expect("run AOT binary");
    assert!(
        out.status.success(),
        "AOT binary should exit 0 without ILO_FORCE_AOT_SIGNAL. stderr={:?}",
        String::from_utf8_lossy(&out.stderr),
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "1",
        "expected stdout `1`. stderr={:?}",
        String::from_utf8_lossy(&out.stderr),
    );
    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&bin);
}
