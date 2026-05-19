// Regression tests pinning the `rand` / `rnd` alias contract.
//
// `rand` is a long-form alias for canonical `rnd` (random builtin). The
// motivating trap is the round-vs-random confusion event-trace-analyser
// rerun11 surfaced: agents read `rnd` as "drop-vowels of round" and reach
// for it when they want rounding. Aliasing `rand` -> `rnd` gives agents
// the universal short-form they reach for from C / Python / Rust / Go /
// JS, leaving `rou` (alias `round`) as the unambiguous rounding builtin.
//
// These tests pin:
//   - `rand` resolves to canonical `rnd` via `ast::resolve_alias`.
//   - `rand` (zero-arg) and `rand a b` (two-arg) both dispatch correctly
//     across every engine.
//   - `rand` is not a canonical builtin name (long form only).
//   - `rou` (round, the disambiguation partner) keeps working.

use ilo::ast::resolve_alias;
use ilo::builtins::Builtin;
use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} {src:?} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm"];

// Zero-arg: random float in [0, 1). Probe-shape because we can't assert
// the value; both `rnd` and `rand` must produce a value in range.
const RND_ZERO: &str = "f>b;x=rnd;a=?>=x 0 1 0;b=?<x 1 1 0;t=*a b;=t 1";
const RAND_ZERO: &str = "f>b;x=rand;a=?>=x 0 1 0;b=?<x 1 1 0;t=*a b;=t 1";

// Two-arg: random integer in [a, b] inclusive. Sample 30 times to keep
// probe noise low while not blowing up cross-engine wall time.
const RND_RANGE: &str =
    "f>b;ok=1;@i 0..30{r=rnd 1 6;lo=?>=r 1 1 0;hi=?<=r 6 1 0;in=*lo hi;ok=*ok in};=ok 1";
const RAND_RANGE: &str =
    "f>b;ok=1;@i 0..30{r=rand 1 6;lo=?>=r 1 1 0;hi=?<=r 6 1 0;in=*lo hi;ok=*ok in};=ok 1";

#[test]
fn rand_alias_resolves_to_rnd() {
    assert_eq!(
        resolve_alias("rand"),
        Some("rnd"),
        "`rand` must resolve to canonical `rnd`"
    );
}

#[test]
fn rand_is_not_a_canonical_builtin() {
    // `rand` is a long-form alias only. The canonical name is `rnd`.
    assert!(
        Builtin::from_name("rand").is_none(),
        "`rand` must not be a canonical name; it is a long-form alias of `rnd`"
    );
    assert!(
        Builtin::from_name("rnd").is_some(),
        "`rnd` must be a canonical builtin"
    );
}

#[test]
fn random_still_resolves_to_rnd() {
    // Don't regress the pre-existing alias when adding `rand`.
    assert_eq!(
        resolve_alias("random"),
        Some("rnd"),
        "`random` must still resolve to canonical `rnd`"
    );
}

#[test]
fn rand_zero_arg_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, RAND_ZERO, "f");
        assert_eq!(
            out, "true",
            "{engine}: rand (zero-arg) should produce float in [0, 1)"
        );
    }
}

#[test]
fn rnd_zero_arg_cross_engine_regression() {
    // Canonical form must keep working alongside the new alias.
    for engine in ENGINES_ALL {
        let out = run(engine, RND_ZERO, "f");
        assert_eq!(out, "true", "{engine}: rnd (zero-arg) regression");
    }
}

#[test]
fn rand_two_arg_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, RAND_RANGE, "f");
        assert_eq!(
            out, "true",
            "{engine}: rand a b should produce integer in [a, b]"
        );
    }
}

#[test]
fn rnd_two_arg_cross_engine_regression() {
    for engine in ENGINES_ALL {
        let out = run(engine, RND_RANGE, "f");
        assert_eq!(out, "true", "{engine}: rnd a b regression");
    }
}

#[test]
fn rand_alias_emits_hint() {
    // Using `rand` should fire the canonical-form hint pointing at `rnd`,
    // same as every other long-form alias (`length` -> `len` etc).
    let out = ilo()
        .args(["f>b;x=rand;=x x", "f"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("hint") && stderr.contains("rand") && stderr.contains("rnd"),
        "expected `rand` -> `rnd` alias hint, got stderr: {stderr}"
    );
}

#[test]
fn rou_still_canonical_for_rounding() {
    // Disambiguation partner: `rou` (alias `round`) is the rounder.
    // If anyone ever flips the alias table so `round` points back at
    // `rnd`, this test screams.
    assert_eq!(
        resolve_alias("round"),
        Some("rou"),
        "`round` must resolve to canonical `rou` (the rounder), not `rnd` (random)"
    );
    for engine in ENGINES_ALL {
        let out = run(engine, "f>n;rou 3.7", "f");
        assert_eq!(out, "4", "{engine}: rou 3.7 should round to 4");
        let out = run(engine, "f>n;round 3.7", "f");
        assert_eq!(out, "4", "{engine}: round 3.7 (alias) should round to 4");
    }
}
