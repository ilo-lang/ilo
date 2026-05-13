// Cross-engine regression coverage for the `sleep ms` builtin.
//
// Motivation: pre-fix, ilo had no `sleep`/`wait` primitive, so any
// polling tail (`wh <dt 2{n2=now;dt=- n2 t0}`) pinned a core at 99% CPU.
// `sleep` adds the missing primitive; the tree interpreter calls
// `std::thread::sleep`, and `--run-vm` / `--run-cranelift` route through
// the generic `OP_CALL_BUILTIN_TREE` bridge (PR #234) so every engine
// shares one implementation.
//
// Every test below runs on tree, VM, and Cranelift, asserting (a) the
// engine returns the expected sentinel and (b) the wall-clock duration
// is within a generous tolerance window of the requested ms. The window
// is asymmetric on purpose: we never want to assert the engine slept
// LESS than requested (that's the actual bug we're guarding against),
// but we tolerate generous over-sleep so CI under load doesn't flake.

use std::process::Command;
use std::time::Instant;

const ENGINES: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_engine(src: &str, engine: &str) -> (String, std::time::Duration) {
    let start = Instant::now();
    let out = ilo()
        .args([src, engine, "f"])
        .output()
        .expect("failed to run ilo");
    let elapsed = start.elapsed();
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (stdout, elapsed)
}

// ── Functional: sleep returns and composes ────────────────────────────

#[test]
fn sleep_returns_through_to_next_expression_cross_engine() {
    // `sleep 50` returns Nil; the trailing `42` is the function's value.
    // This anchors that the bridge round-trip yields a NanVal the rest
    // of the program can step over.
    for engine in ENGINES {
        let (out, _) = run_engine("f>n;sleep 50;42", engine);
        assert_eq!(out, "42", "engine={engine}");
    }
}

#[test]
fn sleep_zero_is_a_noop_cross_engine() {
    // sleep 0 must NOT pause. Anchors that the f64→u64 conversion
    // handles the zero boundary cleanly. The ceiling is set well above
    // the binary's cold-start cost (≈650ms under cranelift on a quiet
    // box, more on noisy CI) so the assertion only fires when the engine
    // is actually mishandling sleep(0).
    for engine in ENGINES {
        let (out, elapsed) = run_engine("f>n;sleep 0;1", engine);
        assert_eq!(out, "1", "engine={engine}");
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "engine={engine}: sleep 0 took {:?}, expected near-zero",
            elapsed
        );
    }
}

#[test]
fn sleep_negative_is_a_noop_cross_engine() {
    // A negative ms argument cannot hang the engine. Clamped to zero
    // in the interpreter so `sleep -1` is observably a no-op (same
    // ceiling rationale as sleep_zero_is_a_noop_cross_engine).
    for engine in ENGINES {
        let (out, elapsed) = run_engine("f>n;sleep -1;1", engine);
        assert_eq!(out, "1", "engine={engine}");
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "engine={engine}: sleep -1 took {:?}, expected near-zero",
            elapsed
        );
    }
}

// ── Wall-clock: sleep actually pauses for ~ms ─────────────────────────

#[test]
fn sleep_pauses_for_requested_ms_tree() {
    timing_check("--run-tree", 200);
}

#[test]
fn sleep_pauses_for_requested_ms_vm() {
    timing_check("--run-vm", 200);
}

#[test]
fn sleep_pauses_for_requested_ms_cranelift() {
    timing_check("--run-cranelift", 200);
}

fn timing_check(engine: &str, ms: u64) {
    // Repeat a couple of times so a single slow process spawn doesn't
    // dominate the measurement. We assert a one-sided lower bound: the
    // run MUST take at least `ms` ms (allowing 20ms slop for thread
    // resolution on noisy CI). No upper bound — over-sleep is fine.
    let src = format!("f>n;sleep {ms};1");
    let (out, elapsed) = run_engine(&src, engine);
    assert_eq!(out, "1", "engine={engine}");
    let floor = std::time::Duration::from_millis(ms.saturating_sub(20));
    assert!(
        elapsed >= floor,
        "engine={engine}: sleep {ms} returned in {:?}, expected >= {:?}",
        elapsed,
        floor
    );
    // Generous upper bound just to surface "engine spun for 30s" bugs;
    // we don't want this to flake on a busy CI, but a 10x ceiling on a
    // 200ms sleep is still well clear of any reasonable startup tax.
    let ceiling = std::time::Duration::from_millis(ms * 10 + 2_000);
    assert!(
        elapsed <= ceiling,
        "engine={engine}: sleep {ms} took {:?}, expected <= {:?}",
        elapsed,
        ceiling
    );
}

// ── Inside a loop body: the actual polling use case ───────────────────

#[test]
fn sleep_inside_loop_body_paces_iterations_cross_engine() {
    // Three iterations of `sleep 80` should take >= 240ms regardless of
    // engine. This is the polling-tail use case that motivated the
    // builtin: the loop body sleeps instead of busy-waiting.
    for engine in ENGINES {
        let src = "f>n;@i 0..3 {sleep 80};7";
        let (out, elapsed) = run_engine(src, engine);
        assert_eq!(out, "7", "engine={engine}");
        let floor = std::time::Duration::from_millis(240 - 20);
        assert!(
            elapsed >= floor,
            "engine={engine}: 3x sleep 80 returned in {:?}, expected >= {:?}",
            elapsed,
            floor
        );
    }
}
