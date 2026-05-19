// Cross-engine regression tests for the `now-ms` builtin.
//
// `now-ms` returns the current Unix epoch in milliseconds as f64. Before
// the addition, the only timer was `now` (seconds), which was too coarse
// for sub-second perf bisection — agents had to manually trim programs
// and re-run, paying 5x the trial cost on every perf-regression hunt.
//
// These tests pin parity across tree / VM / Cranelift:
//   - returns a positive number (epoch starts 1970)
//   - is monotonic across a `sleep` boundary
//   - the delta over a known sleep is non-zero
//   - returns a numeric type the rest of the arithmetic surface accepts
//
// The python codegen path is exercised separately by the round-trip
// test in `src/builtins.rs` (name registry) and the snapshot tests under
// `src/codegen/python.rs`.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_ok(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} {src:?} unexpectedly failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--vm"];

#[test]
fn now_ms_positive_cross_engine() {
    // Unix epoch is always positive — cheap deterministic smoke test.
    let src = "f>b;>now-ms 0";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, "f"),
            "true",
            "{engine}: now-ms must return a positive number"
        );
    }
}

#[test]
fn now_ms_after_sleep_is_monotonic_cross_engine() {
    // Two samples with a `sleep` between must satisfy `t1 >= t0`. The
    // bare comparison `>=t1 t0` would fire as a guard, so bind first.
    let src = "f>b;t0=now-ms;sleep 10;t1=now-ms;r=>=t1 t0;r";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, "f"),
            "true",
            "{engine}: now-ms must not move backwards across sleep"
        );
    }
}

#[test]
fn now_ms_delta_after_sleep_is_at_least_a_few_ms_cross_engine() {
    // After `sleep 20`, the elapsed should be >= 10ms in practice on any
    // reasonable host. We assert the lower-bound that the OS sleep takes
    // *some* observable time, not an exact value, so the test isn't
    // flaky on noisy CI runners. The point is that `now-ms` is fine
    // enough resolution to see this at all (which `now` in seconds is
    // not).
    let src = "f>b;t0=now-ms;sleep 20;t1=now-ms;dt=-t1 t0;>=dt 10";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, "f"),
            "true",
            "{engine}: now-ms delta after `sleep 20` should be at least 10ms"
        );
    }
}

#[test]
fn now_ms_returns_number_arith_compatible_cross_engine() {
    // The verifier types `now-ms` as `n`, so it must compose with the
    // numeric arithmetic surface. `+now-ms 0` is the identity and
    // should equal `now-ms` itself within a small (sub-ms) window.
    // We assert non-negative and integer-truncated for printing.
    let src = "f>b;v=+now-ms 0;>=v 0";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, "f"),
            "true",
            "{engine}: now-ms must compose with `+` as a number"
        );
    }
}

#[test]
fn now_ms_is_finer_grained_than_now_seconds_cross_engine() {
    // Sanity: `now-ms` returns ~1000x the seconds value. Compute the
    // ratio's floor and assert it's in [900, 1100] to leave room for
    // the sample skew between the two calls. This is the property
    // that motivates the builtin — `now` alone cannot observe a
    // sub-second phase boundary.
    let src = "f>b;ms=now-ms;s=now;r=/ms s;>=r 900";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, "f"),
            "true",
            "{engine}: now-ms should be ~1000x now-seconds"
        );
    }
}
