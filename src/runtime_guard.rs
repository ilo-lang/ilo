//! Production-safety guards for `ilo run`.
//!
//! A persona run (mandelbrot, 2026-05-20) missed a `col=col+1` loop increment
//! and spun in an infinite loop, producing 165 MB of stdout before the harness
//! killed the process. The agent had no useful signal to learn from — the
//! transcript was just a wall of dots. This module exists so the next runaway
//! program aborts cleanly with a diagnostic (`ILO-R016` for wall-clock,
//! `ILO-R017` for stdout bytes) that names the budget, the override flag, and
//! a hint about the most likely cause.
//!
//! ## Surface
//!
//! - [`install`] — call once from `fn main` after parsing `--max-runtime` /
//!   `--max-output-bytes`. Spawns the watchdog thread.
//! - [`record_output`] — call at every print site in every engine (tree, VM,
//!   the small set of native `println!`s in `prnt`/result-print paths).
//!   Increments a process-wide byte counter; aborts via [`abort_with`] if the
//!   total exceeds the budget.
//! - [`abort_with`] — write a structured diagnostic to stderr (JSON line in
//!   non-TTY contexts, plain text otherwise) and `process::exit(1)`. Async-
//!   signal-safe enough for the watchdog thread.
//!
//! The defaults (60 s, ~100 MB) are the same shape Zero ships: high enough
//! that no legitimate program is bothered, low enough that a runaway loop
//! gets killed inside a single agent turn.
//!
//! Both budgets are off when [`install`] is not called — the library still
//! works as a library; only `ilo run` opts in.

use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

/// Default wall-clock budget for `ilo run`: 60 s. Anything longer and the
/// agent has almost certainly produced a runaway loop. CLI override:
/// `--max-runtime <seconds>` (0 disables).
pub const DEFAULT_MAX_RUNTIME_SECS: u64 = 60;

/// Default stdout budget for `ilo run`: ~100 MB (100 * 1024 * 1024 bytes).
/// The mandelbrot persona produced 165 MB of dots before the harness
/// noticed — 100 MB stops that runaway long before it fills disk or hits
/// the agent transcript limit. CLI override: `--max-output-bytes <bytes>`
/// (0 disables).
pub const DEFAULT_MAX_OUTPUT_BYTES: u64 = 100 * 1024 * 1024;

/// 0 = guard not installed / disabled.
static MAX_RUNTIME_MS: AtomicU64 = AtomicU64::new(0);
static MAX_OUTPUT_BYTES: AtomicU64 = AtomicU64::new(0);
static OUTPUT_BYTES_USED: AtomicU64 = AtomicU64::new(0);
static OUTPUT_MODE: AtomicUsize = AtomicUsize::new(0); // 0 ansi/text, 1 json
static ABORTED: AtomicBool = AtomicBool::new(false);

/// Output-mode hint for [`abort_with`]. We can't depend on `OutputMode` here
/// without a circular import, so we mirror the relevant bit (json vs text).
#[derive(Copy, Clone)]
pub enum AbortMode {
    Text,
    Json,
}

/// Install the guard. `max_runtime` of 0 disables the wall-clock cap;
/// `max_output_bytes` of 0 disables the output cap.
///
/// Call once from `fn main` per `ilo run`. Safe to no-op (zero / zero) so
/// non-run subcommands skip the watchdog entirely.
pub fn install(max_runtime: Duration, max_output_bytes: u64, mode: AbortMode) {
    let ms = max_runtime.as_millis().min(u64::MAX as u128) as u64;
    MAX_RUNTIME_MS.store(ms, Ordering::Relaxed);
    MAX_OUTPUT_BYTES.store(max_output_bytes, Ordering::Relaxed);
    OUTPUT_BYTES_USED.store(0, Ordering::Relaxed);
    OUTPUT_MODE.store(
        match mode {
            AbortMode::Text => 0,
            AbortMode::Json => 1,
        },
        Ordering::Relaxed,
    );
    ABORTED.store(false, Ordering::Relaxed);

    if ms > 0 {
        std::thread::Builder::new()
            .name("ilo-runtime-watchdog".to_string())
            .spawn(move || {
                let start = std::time::Instant::now();
                loop {
                    if ABORTED.load(Ordering::Relaxed) {
                        return;
                    }
                    let elapsed_ms = start.elapsed().as_millis() as u64;
                    if elapsed_ms >= ms {
                        abort_with(
                            "ILO-R016",
                            &format!(
                                "wall-clock runtime exceeded {} ms (--max-runtime {})",
                                ms,
                                ms / 1000
                            ),
                            "infinite loop is the most common cause — check loop variables increment, recursion has a base case, or pass `--max-runtime N` if a legitimate program needs longer.",
                        );
                    }
                    // 100 ms granularity keeps the kill latency tight without
                    // burning a core spinning.
                    std::thread::sleep(Duration::from_millis(100));
                }
            })
            .expect("spawn watchdog thread");
    }
}

/// Record `n` bytes written to stdout. Aborts via [`abort_with`] if the
/// total exceeds the configured budget. Cheap when no budget is set
/// (one relaxed atomic load).
pub fn record_output(n: usize) {
    let cap = MAX_OUTPUT_BYTES.load(Ordering::Relaxed);
    if cap == 0 {
        return;
    }
    let total = OUTPUT_BYTES_USED.fetch_add(n as u64, Ordering::Relaxed) + n as u64;
    if total > cap {
        abort_with(
            "ILO-R017",
            &format!("stdout output exceeded {cap} bytes (--max-output-bytes)"),
            "a loop printing without a break or increment is the most common cause — check `prnt` calls inside `wh`/`fa` bodies. raise the cap with `--max-output-bytes N` if a legitimate program needs more.",
        );
    }
}

/// Write a structured diagnostic to stderr and exit the process with code 1.
///
/// Idempotent: the first caller wins; concurrent callers (e.g. watchdog
/// firing the same instant a print-site overflows) silently return so the
/// stderr output stays a single coherent message.
pub fn abort_with(code: &str, message: &str, hint: &str) -> ! {
    if ABORTED.swap(true, Ordering::SeqCst) {
        // Another thread is already shutting down — park forever, the
        // first caller will exit() us.
        loop {
            std::thread::sleep(Duration::from_secs(60));
        }
    }
    let stderr = std::io::stderr();
    let mut h = stderr.lock();
    if OUTPUT_MODE.load(Ordering::Relaxed) == 1 {
        let json = serde_json::json!({
            "error": {
                "code": code,
                "message": message,
                "hint": hint,
            }
        });
        let _ = writeln!(h, "{json}");
    } else {
        let _ = writeln!(h, "error[{code}]: {message}");
        let _ = writeln!(h, "  hint: {hint}");
    }
    let _ = h.flush();
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_output_noop_when_uninstalled() {
        // Clean slate
        MAX_OUTPUT_BYTES.store(0, Ordering::Relaxed);
        OUTPUT_BYTES_USED.store(0, Ordering::Relaxed);
        record_output(1_000_000);
        assert_eq!(OUTPUT_BYTES_USED.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn record_output_accumulates_under_budget() {
        MAX_OUTPUT_BYTES.store(1024, Ordering::Relaxed);
        OUTPUT_BYTES_USED.store(0, Ordering::Relaxed);
        ABORTED.store(false, Ordering::Relaxed);
        record_output(100);
        record_output(200);
        assert_eq!(OUTPUT_BYTES_USED.load(Ordering::Relaxed), 300);
        // Reset so other tests don't see this state.
        MAX_OUTPUT_BYTES.store(0, Ordering::Relaxed);
        OUTPUT_BYTES_USED.store(0, Ordering::Relaxed);
    }
}
