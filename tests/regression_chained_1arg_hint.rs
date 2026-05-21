// Cross-engine regression test: 2-arg numeric builtins passed as the
// argument of a unary builtin used to misparse. The outer unary call
// (e.g. `abs`) ran in the greedy-args path, while `parse_call_arg`
// only recursed into a nested call when the inner ident had a known
// arity in `fn_arity`. Several pure-numeric builtins (`rndn`, `atan2`,
// `fmod`, `clamp`, and the additional unary trig forms) were missing
// from that table, so the parser would peel `rndn` off as a bare Ref
// and let the outer call swallow the remaining operands:
//
//     abs rndn 0 1   →   abs(rndn, 0, 1)   ❌ arity mismatch
//
// The fix registers the missing arities in `builtin_arity_tables()`
// so the nested-call branch fires and parses these chains correctly:
//
//     abs rndn 0 1   →   abs(rndn(0, 1))   ✅
//
// Hit by linear-regression persona (pending #5ao). The workaround
// used to be `abs (rndn 0 1)` — agents shouldn't need the parens.
//
// These tests pin the behaviour across every public engine so any
// future grammar refactor that drops the structural fix shows up
// immediately.

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

// ── pow already worked (pinned in table) — sanity that we didn't break it ─

#[test]
fn abs_pow_int_int_cross_engine() {
    let src = "g>n;abs pow 2 3";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "g"), "8", "{engine}: abs pow 2 3");
    }
}

// ── new: 2-arg numeric builtins chain without parens ────────────────────

#[test]
fn abs_atan2_int_int_cross_engine() {
    // atan2(1, 1) = pi/4; abs(pi/4) = pi/4.
    let src = "g>n;abs atan2 1 1";
    for engine in ENGINES_ALL {
        let got = run_ok(engine, src, "g");
        assert!(
            got.starts_with("0.7853981"),
            "{engine}: abs atan2 1 1 → {got} (want ≈ pi/4)"
        );
    }
}

#[test]
fn abs_fmod_int_int_cross_engine() {
    let src = "g>n;abs fmod 10 3";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "g"), "1", "{engine}: abs fmod 10 3");
    }
}

#[test]
fn sqrt_atan2_int_int_cross_engine() {
    // Different unary outer to prove the fix isn't `abs`-specific.
    let src = "g>n;sqrt atan2 1 1";
    for engine in ENGINES_ALL {
        let got = run_ok(engine, src, "g");
        assert!(
            got.starts_with("0.8862269"),
            "{engine}: sqrt atan2 1 1 → {got} (want sqrt(pi/4))"
        );
    }
}

// ── 3-arg numeric builtin (clamp) chains under a unary outer ─────────────

#[test]
fn abs_clamp_three_args_cross_engine() {
    // clamp(5, 0, 10) = 5; abs(5) = 5.
    let src = "g>n;abs clamp 5 0 10";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "g"), "5", "{engine}: abs clamp 5 0 10");
    }
}

// ── the pending repro shape (rndn — stochastic, just assert success) ─────

#[test]
fn abs_rndn_int_float_cross_engine() {
    // `rndn 0 0.1` was the original failure shape in pending #5ao —
    // an int literal followed by a float literal as the 2-arg pair.
    // Output is stochastic; assert only that parse + execute succeed.
    let src = "g>n;abs rndn 0 0.1";
    for engine in ENGINES_ALL {
        let out = ilo()
            .args([src, engine, "g"])
            .output()
            .expect("failed to run ilo");
        assert!(
            out.status.success(),
            "{engine}: abs rndn 0 0.1 failed: stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        let trimmed = stdout.trim();
        let parsed: f64 = trimmed.parse().unwrap_or_else(|e| {
            panic!("{engine}: abs rndn 0 0.1 produced non-number {trimmed:?} ({e})")
        });
        assert!(
            parsed >= 0.0,
            "{engine}: abs(...) must be >= 0, got {parsed}"
        );
    }
}

// ── original repro shape with binding + binary op ────────────────────────

#[test]
fn rndn_in_arith_expr_cross_engine() {
    // From pending #5ao: `rndn 0 0.1` as part of a larger expression.
    // Was the exact shape that tripped the linear-regression persona.
    let src = "g>n;y=+1 rndn 0 0.1;y";
    for engine in ENGINES_ALL {
        let out = ilo()
            .args([src, engine, "g"])
            .output()
            .expect("failed to run ilo");
        assert!(
            out.status.success(),
            "{engine}: y=+1 rndn 0 0.1 failed: stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

// ── unary trig builtins (log10, log2, tan, asin, acos, atan) ────────────

#[test]
fn log10_log2_tan_chain_cross_engine() {
    // `log10 1000` = 3 and chained under `abs` proves the new unary
    // entries route through the nested-call branch correctly.
    let src = "g>n;abs log10 1000";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "g"), "3", "{engine}: abs log10 1000");
    }
}

#[test]
fn nested_atan2_in_pow_cross_engine() {
    // pow(atan2(1,1), 2) — two 2-arg numerics composed.
    let src = "g>n;pow atan2 1 1 2";
    for engine in ENGINES_ALL {
        let got = run_ok(engine, src, "g");
        let parsed: f64 = got.parse().unwrap_or_else(|e| {
            panic!("{engine}: pow atan2 1 1 2 produced non-number {got:?} ({e})")
        });
        // (pi/4)^2 ≈ 0.6168502750680849
        assert!(
            (parsed - 0.6168502750680849).abs() < 1e-9,
            "{engine}: pow atan2 1 1 2 → {parsed} (want ≈ (pi/4)^2)"
        );
    }
}
