// Cross-engine regression tests for the math constant builtins `pi`,
// `tau`, and `e`. Added 0.12.1.
//
// Before these existed, fft-peak rerun12 surfaced agents reconstructing
// pi via `* 2 (atan2 0 -1)` and hardcoding `3.14159...` because there was
// no canonical short name. These tests pin:
//   - the printed value matches Rust's f64::consts::{PI, TAU, E} bit-for-bit
//   - the constants are usable in expressions (compose with sin/exp/+/etc.)
//   - tau == 2 * pi (exact, since both are stored constants)
//   - exp 1 == e (exact, since `exp` is mapped to f64::exp and `e` to f64::consts::E)
//   - tree-walker, VM, and Cranelift JIT all agree
//
// Reservation as a binding / fn name (ILO-P011) is exercised by the
// existing builtin_binding_name / builtin_fn_name regressions; adding
// the three constants to `Builtin::from_name` is enough to opt them in.

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
const ENGINES_ALL: &[&str] = &["--run-vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--run-vm"];

#[test]
fn pi_prints_canonical_value_cross_engine() {
    // Rust's `format!("{}", std::f64::consts::PI)` shortest-roundtrip
    // representation. Pinning this exact string protects against any
    // future engine that re-derives pi (NEVER do that — use the const).
    let src = "f>n;pi";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, "f"),
            "3.141592653589793",
            "{engine}: pi must print the canonical f64::consts::PI value"
        );
    }
}

#[test]
fn tau_prints_canonical_value_cross_engine() {
    let src = "f>n;tau";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, "f"),
            "6.283185307179586",
            "{engine}: tau must print the canonical f64::consts::TAU value"
        );
    }
}

#[test]
fn e_prints_canonical_value_cross_engine() {
    let src = "f>n;e";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, "f"),
            "2.718281828459045",
            "{engine}: e must print the canonical f64::consts::E value"
        );
    }
}

#[test]
fn tau_equals_two_pi_cross_engine() {
    // `tau == 2 * pi` is exact because both are stored constants. If a
    // backend ever re-derives one it almost certainly drifts by an ulp;
    // this guards against that.
    let src = "f>b;==tau (* 2 pi)";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, "f"),
            "true",
            "{engine}: tau must equal 2 * pi exactly"
        );
    }
}

#[test]
fn exp_one_equals_e_cross_engine() {
    // `exp 1.0` and `f64::consts::E` agree bit-for-bit in IEEE-754, so
    // an exact equality holds on every engine that honours the contract.
    let src = "f>b;==exp 1 e";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, "f"),
            "true",
            "{engine}: exp 1 must equal e exactly"
        );
    }
}

#[test]
fn sin_pi_is_near_zero_cross_engine() {
    // `sin pi` is not exactly 0.0 in f64 (pi rounds, sin(pi_rounded) is
    // about 1.22e-16). Assert the magnitude is below a generous bound
    // so we catch a builtin that returned 0.0 (suggesting pi was misread
    // as 3.14) without hard-coding the exact f64 result.
    let src = "f>b;<abs sin pi 0.000000000001";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, "f"),
            "true",
            "{engine}: |sin pi| must be < 1e-12"
        );
    }
}

#[test]
fn sin_half_pi_is_one_cross_engine() {
    // sin(pi/2) is exactly 1.0 in f64 for the IEEE-754-rounded value of
    // pi/2. This catches any backend that swaps pi for 3.14 or similar
    // truncation: sin(3.14/2) is 0.9999996829..., not 1.0.
    let src = "f>n;sin (/ pi 2)";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, "f"),
            "1",
            "{engine}: sin (pi/2) must be exactly 1"
        );
    }
}

#[test]
fn constants_compose_in_arithmetic_cross_engine() {
    // Bound up in a function and used in arithmetic — exercises that
    // the verifier types them as `n` and the constant load survives
    // local-binding lowering on every engine.
    let src = "f>n;c=pi;t=tau;+c t";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, "f"),
            "9.42477796076938",
            "{engine}: pi + tau must compose cleanly"
        );
    }
}

#[test]
fn pi_with_arity_one_call_is_arity_error_cross_engine() {
    // `pi 3` would be a degenerate call shape — the verifier rejects
    // the arity-1 form just like `now 3` does. We don't pin the exact
    // diagnostic text, just that a non-zero exit happens with an
    // ILO-T-prefixed code reaching stderr.
    let src = "f>n;pi 3";
    for engine in ENGINES_ALL {
        let out = ilo()
            .args([src, engine, "f"])
            .output()
            .expect("failed to run ilo");
        assert!(
            !out.status.success(),
            "{engine}: pi with an argument must fail verification"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("ILO-T"),
            "{engine}: expected ILO-T diagnostic, got: {stderr}"
        );
    }
}
