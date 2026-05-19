// Regression tests pinning the `rand` → `rnd` short-form alias contract.
//
// `rand` is a short-form alias for the canonical `rnd` builtin (random number).
// Added in 0.12.1 because `rand` is the universal short-form for random across
// C / Python / Rust / Go / JS, and personas reach for it first when they want
// a random value. Same shape as the existing `random` → `rnd` long-form alias
// and the `rng` → `range` short-form alias.
//
// Contracts to lock in:
// 1. `rand` resolves to `rnd` at the alias-table level.
// 2. `rand` and `rand a b` execute successfully on every engine (zero-arg and
//    two-arg dispatch both go through the alias rewrite). Output values are
//    random so we only assert successful dispatch + valid numeric output in
//    the expected range.
// 3. `rand` as a binding name or user-function name is rejected at parse time
//    with `ILO-P011`. Without this guard the alias resolver rewrites later
//    Call sites and silently mis-dispatches user bindings.

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

#[test]
fn rnd_remains_the_canonical_name() {
    // `rnd` must stay canonical in the registry; `rand` is only an alias.
    let b = Builtin::from_name("rnd").expect("`rnd` must be a canonical builtin");
    assert_eq!(b.name(), "rnd", "canonical name is `rnd`");
    assert!(
        Builtin::from_name("rand").is_none(),
        "`rand` must not be a canonical name; it is an alias for `rnd`"
    );
    assert_eq!(
        resolve_alias("rand"),
        Some("rnd"),
        "`rand` must resolve to canonical `rnd`"
    );
    // `random` should keep resolving to `rnd` too — pre-existing alias.
    assert_eq!(
        resolve_alias("random"),
        Some("rnd"),
        "`random` must still resolve to canonical `rnd`"
    );
}

#[test]
fn rand_zero_arg_dispatches_cross_engine() {
    // Zero-arg `rand` should parse as Call("rnd",[]) after alias rewrite and
    // produce a float in [0, 1). Random output, so just check it parses to a
    // valid float in range on every engine.
    for engine in ENGINES_ALL {
        let out = run(engine, "f>n;rand", "f");
        let n: f64 = out
            .parse()
            .unwrap_or_else(|_| panic!("{engine}: expected numeric output from `rand`, got {out}"));
        assert!(
            (0.0..1.0).contains(&n),
            "{engine}: rand returned out-of-range value {n}"
        );
    }
}

#[test]
fn rand_two_arg_dispatches_cross_engine() {
    // `rand a b` should parse as Call("rnd",[a,b]) after alias rewrite and
    // produce a number in [a, b] (inclusive). Check on every engine.
    for engine in ENGINES_ALL {
        let out = run(engine, "f>n;rand 10 20", "f");
        let n: f64 = out
            .parse()
            .unwrap_or_else(|_| panic!("{engine}: expected numeric output, got {out}"));
        assert!(
            (10.0..=20.0).contains(&n),
            "{engine}: rand 10 20 returned out-of-range value {n}"
        );
    }
}

#[test]
fn rand_in_binding_position_cross_engine() {
    // Bind `rand` to a local, use the local — exercises the Ref-position alias
    // rewrite (Expr::Ref("rand") → Expr::Ref("rnd")) and the zero-arg-call
    // synthesis at parse time. If either path regresses the binding silently
    // captures a function reference instead of the float value.
    for engine in ENGINES_ALL {
        let out = run(engine, "f>n;x=rand;x", "f");
        let n: f64 = out
            .parse()
            .unwrap_or_else(|_| panic!("{engine}: expected numeric output, got {out}"));
        assert!(
            (0.0..1.0).contains(&n),
            "{engine}: x=rand;x returned out-of-range value {n}"
        );
    }
}

#[test]
fn rand_rejected_as_binding_name() {
    // Without the ILO-P011 guard, `rand=5;rand` silently rewrites the later
    // Call to `rnd` and mis-dispatches around the user binding.
    let out = ilo()
        .args(["main>n;rand=5;rand"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "expected `rand=5` to fail at parse time"
    );
    assert!(
        stderr.contains("ILO-P011"),
        "expected ILO-P011 reserved-name error, got: {stderr}"
    );
    assert!(
        stderr.contains("rand") && stderr.contains("rnd"),
        "error must name the alias and the canonical builtin, got: {stderr}"
    );
}

#[test]
fn rand_rejected_as_user_function_name() {
    let out = ilo()
        .args(["rand x:n>n;+x 1"])
        .output()
        .expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "expected `rand x:n>n;...` to fail at parse time"
    );
    assert!(
        stderr.contains("ILO-P011"),
        "expected ILO-P011 reserved-name error, got: {stderr}"
    );
}

#[test]
fn rand_emits_canonical_hint() {
    // First use of `rand` should hint toward `rnd`. Matches the existing
    // `random` / `rng` alias-hint contract.
    let out = ilo()
        .args(["f>n;rand", "--vm", "f"])
        .output()
        .expect("failed to run ilo");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("rand") && combined.contains("rnd"),
        "expected alias hint mentioning `rand` and `rnd`, got: {combined}"
    );
}
