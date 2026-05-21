//! Integration tests for the `ilo run` production-safety guards.
//!
//! Origin: mandelbrot persona (2026-05-20) ran an infinite loop and produced
//! 165 MB of stdout before the harness killed it. These tests pin the
//! `--max-runtime` (`ILO-R016`) and `--max-output-bytes` (`ILO-R017`) guards
//! across every engine the bare-positional dispatch can pick.
//!
//! Each test uses a subprocess because the guard calls `process::exit(1)`
//! after writing a structured diagnostic to stderr — there is no graceful
//! return path, by design (the alternative is to thread a cancellation
//! token through every engine's eval loop, which is a much bigger change
//! for a guard that only fires on already-broken programs).

use std::process::Command;
use std::time::{Duration, Instant};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Run ilo with the given args; return (success, stdout, stderr, wall-clock).
fn run_args(args: &[&str]) -> (bool, String, String, Duration) {
    let start = Instant::now();
    let out = ilo()
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn ilo: {e}"));
    let elapsed = start.elapsed();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    (out.status.success(), stdout, stderr, elapsed)
}

// ── --max-runtime ─────────────────────────────────────────────────────────────

/// Default engine (VM): infinite loop without output aborts on the runtime
/// budget with ILO-R016. We pass `--max-runtime 1` so the test finishes
/// quickly; the watchdog polls at 100 ms granularity, so the real wall
/// clock should be under ~1.5 s with plenty of slack for CI.
#[test]
fn infinite_loop_default_engine_aborts_on_max_runtime() {
    let (ok, _stdout, stderr, elapsed) = run_args(&[
        "--max-runtime",
        "1",
        "--json",
        "main>n;n=0;wh true{n=+n 1};n",
    ]);
    assert!(!ok, "should abort with non-zero exit");
    assert!(
        stderr.contains("ILO-R016"),
        "expected ILO-R016 in stderr, got: {stderr}"
    );
    assert!(
        stderr.contains("--max-runtime"),
        "diagnostic should name the override flag, got: {stderr}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "watchdog should kill within ~1.5 s, took {elapsed:?}"
    );
}

/// JIT engine path. Without explicit cap on this build, infinite loops
/// would otherwise spin forever. Note we pin to the cranelift feature so
/// this only runs on feature-enabled builds (the default).
#[cfg(feature = "cranelift")]
#[test]
fn infinite_loop_jit_engine_aborts_on_max_runtime() {
    let (ok, _stdout, stderr, elapsed) = run_args(&[
        "--max-runtime",
        "1",
        "--json",
        "main>n;n=0;wh true{n=+n 1};n",
        "--jit",
    ]);
    assert!(!ok);
    assert!(stderr.contains("ILO-R016"), "stderr: {stderr}");
    assert!(elapsed < Duration::from_secs(5), "elapsed={elapsed:?}");
}

// ── --max-output-bytes ────────────────────────────────────────────────────────

/// Default engine (VM): a loop printing without termination aborts with
/// ILO-R017 once the byte budget is exhausted. Using 200 bytes so the
/// test takes microseconds.
///
/// The body returns `0` after the (unreachable) loop so `>n` typechecks —
/// the loop itself spins forever in practice, but the verifier doesn't
/// know that.
#[test]
fn runaway_prnt_loop_default_engine_aborts_on_max_output_bytes() {
    let (ok, stdout, stderr, _elapsed) = run_args(&[
        "--max-output-bytes",
        "200",
        "--json",
        "main>n;n=0;wh true{prnt n;n=+n 1};0",
    ]);
    assert!(!ok, "should abort with non-zero exit");
    assert!(
        stderr.contains("ILO-R017"),
        "expected ILO-R017 in stderr, got: {stderr}"
    );
    assert!(
        stderr.contains("--max-output-bytes"),
        "diagnostic should name the override flag, got: {stderr}"
    );
    // stdout should have a bit of content (the partial print before the
    // overflow) but be capped well below the runaway baseline. The exact
    // amount depends on the timing of the overflow check; we just assert
    // it's bounded.
    assert!(
        stdout.len() < 4096,
        "stdout should be capped, got {} bytes",
        stdout.len()
    );
}

/// JIT engine path. Calls go through `jit_prt` which also accounts.
#[cfg(feature = "cranelift")]
#[test]
fn runaway_prnt_loop_jit_engine_aborts_on_max_output_bytes() {
    let (ok, _stdout, stderr, _elapsed) = run_args(&[
        "--max-output-bytes",
        "200",
        "--json",
        "main>n;n=0;wh true{prnt n;n=+n 1};0",
        "--jit",
    ]);
    assert!(!ok);
    assert!(stderr.contains("ILO-R017"), "stderr: {stderr}");
}

// ── happy path: well-behaved programs are unaffected ──────────────────────────

/// A well-behaved program well under both budgets runs to completion.
/// Guards a future change from accidentally tightening the cap for normal
/// use.
#[test]
fn well_behaved_program_unaffected_by_default_guards() {
    let (ok, stdout, stderr, _elapsed) = run_args(&["main>n;prnt 42;0"]);
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.contains("42"), "stdout: {stdout}");
}

/// Setting `--max-runtime 0` disables the wall-clock cap entirely. Useful
/// for batch/training runs that the operator already knows are long. We
/// can't actually verify "ran forever" so we settle for "ran a tight loop
/// 100k times under 10 s with no abort".
#[test]
fn max_runtime_zero_disables_guard() {
    let (ok, _stdout, stderr, _elapsed) =
        run_args(&["--max-runtime", "0", "main>n;n=0;wh <n 100000{n=+n 1};n"]);
    assert!(ok, "stderr: {stderr}");
}
