// Regression tests pinning the `rng` → `range` short-form alias contract.
//
// `rng` is a short-form alias for the canonical `range` builtin, added after
// nlp-engineer rerun8 reached for `rng` first when reaching for a numeric
// range. The alias is the inverse direction of the usual long-form alias
// (e.g. `length` → `len`): here the canonical is longer (`range`) and the
// alias is shorter (`rng`).
//
// Two contracts to lock in:
// 1. `rng a b` resolves to `range a b` identically across every engine.
// 2. `rng` as a binding name or user-function name is rejected at parse
//    time with `ILO-P011`. Without this guard, the alias resolver rewrites
//    later Call sites and silently mis-dispatches user bindings.

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
const ENGINES_ALL: &[&str] = &["--vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--vm"];

const RNG_SRC: &str = "f>n;sum rng 0 5";
const RANGE_SRC: &str = "f>n;sum range 0 5";

#[test]
fn rng_alias_resolves_to_range_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, RNG_SRC, "f");
        assert_eq!(out, "10", "{engine}: sum rng 0 5 should equal 0+1+2+3+4");
    }
}

#[test]
fn rng_and_range_produce_identical_output_cross_engine() {
    for engine in ENGINES_ALL {
        let a = run(engine, RNG_SRC, "f");
        let b = run(engine, RANGE_SRC, "f");
        assert_eq!(a, b, "{engine}: rng and range diverged");
    }
}

#[test]
fn range_remains_the_canonical_name() {
    // `range` must stay the canonical name in the registry; `rng` is only an
    // alias that resolves to it.
    let b = Builtin::from_name("range").expect("`range` must be a canonical builtin");
    assert_eq!(b.name(), "range", "canonical name is `range`");
    assert!(
        Builtin::from_name("rng").is_none(),
        "`rng` must not be a canonical name; it is an alias for `range`"
    );
    assert_eq!(
        resolve_alias("rng"),
        Some("range"),
        "`rng` must resolve to canonical `range`"
    );
}

#[test]
fn rng_rejected_as_binding_name() {
    // Without the ILO-P011 guard, `rng=5;rng 0 3` silently rewrites the later
    // Call to `range 0 3` and mis-dispatches around the user binding.
    let out = ilo()
        .args(["main>n;rng=5;rng"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "expected `rng=5` to fail at parse time"
    );
    assert!(
        stderr.contains("ILO-P011"),
        "expected ILO-P011 reserved-name error, got: {stderr}"
    );
    assert!(
        stderr.contains("rng") && stderr.contains("range"),
        "error must name the alias and the canonical builtin, got: {stderr}"
    );
}

#[test]
fn rng_rejected_as_user_function_name() {
    let out = ilo()
        .args(["rng x:n>n;+x 1"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "expected `rng x:n>n;...` to fail at parse time"
    );
    assert!(
        stderr.contains("ILO-P011"),
        "expected ILO-P011 reserved-name error, got: {stderr}"
    );
}
