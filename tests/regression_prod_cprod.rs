// Cross-engine regression tests for `prod` and `cprod`.
//
// `prod xs:L n > n` — product of all elements; empty list = 1 (multiplicative identity).
// `cprod xs:L n > L n` — running product; mirrors `cumsum` but multiplies.
//
// Before this fix, neither builtin existed in ilo at all.

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

fn run_err(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} {src:?} unexpectedly succeeded: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn parse_list(s: &str) -> Vec<f64> {
    let inner = s.trim().trim_start_matches('[').trim_end_matches(']');
    if inner.trim().is_empty() {
        return vec![];
    }
    inner
        .split(',')
        .map(|t| t.trim().parse::<f64>().expect("expected numeric element"))
        .collect()
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--run-vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--run-vm"];

// ── prod ──────────────────────────────────────────────────────────────

#[test]
fn prod_happy_path_cross_engine() {
    let src = "f>n;prod [1, 2, 3, 4]";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "f"), "24", "{engine}: prod [1..4] = 24");
    }
}

#[test]
fn prod_floats_cross_engine() {
    let src = "f>n;prod [1.5, 2.0, 4.0]";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "f"), "12", "{engine}: prod floats");
    }
}

#[test]
fn prod_singleton_cross_engine() {
    let src = "f>n;prod [42]";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "f"), "42", "{engine}: prod [42]");
    }
}

#[test]
fn prod_empty_returns_one_cross_engine() {
    // Multiplicative identity: prod [] = 1 (mirrors sum [] = 0).
    let src = "f>n;prod []";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "f"), "1", "{engine}: prod [] = 1");
    }
}

#[test]
fn prod_negative_numbers_cross_engine() {
    let src = "f>n;prod [-1, 2, -3]";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "f"), "6", "{engine}: prod negatives");
    }
}

#[test]
fn prod_zero_in_list_cross_engine() {
    let src = "f>n;prod [1, 2, 0, 4]";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "f"), "0", "{engine}: prod with zero");
    }
}

#[test]
fn prod_non_list_errors_cross_engine() {
    let src = "f>n;prod 42";
    for engine in ENGINES_ALL {
        let err = run_err(engine, src, "f");
        assert!(
            err.contains("prod") || err.contains("list"),
            "{engine}: expected prod/list in error, got: {err}"
        );
    }
}

#[test]
fn prod_non_numeric_element_errors_cross_engine() {
    let src = r#"f>n;prod ["a", "b"]"#;
    for engine in ENGINES_ALL {
        let err = run_err(engine, src, "f");
        assert!(
            err.contains("prod") || err.contains("number"),
            "{engine}: expected prod/number in error, got: {err}"
        );
    }
}

#[test]
fn prod_arithmetic_on_result_cross_engine() {
    // Exercises the F64-shadow-refresh path in the Cranelift JIT.
    // OP_PROD must refresh the F64 shadow so a subsequent OP_MUL_NN / OP_ADD_NN
    // does not read a stale zero.
    let src = "f>n;p=prod [2, 3, 4];+p 1";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "f"), "25", "{engine}: prod + 1");
    }
}

// ── cprod ───────────────────────────────────────────────────────────

fn check_cprod(src: &str, expected: &[f64]) {
    for engine in ENGINES_ALL {
        let got = parse_list(&run_ok(engine, src, "f"));
        assert_eq!(
            got.len(),
            expected.len(),
            "engine={engine} src=`{src}`: length mismatch (got {got:?}, expected {expected:?})"
        );
        for (i, (a, e)) in got.iter().zip(expected.iter()).enumerate() {
            assert!(
                (a - e).abs() < 1e-10,
                "engine={engine} src=`{src}`: index {i} got {a}, expected {e}"
            );
        }
    }
}

#[test]
fn cprod_basic() {
    check_cprod("f>L n;cprod [1,2,3,4]", &[1.0, 2.0, 6.0, 24.0]);
}

#[test]
fn cprod_empty() {
    check_cprod("f>L n;cprod []", &[]);
}

#[test]
fn cprod_singleton() {
    check_cprod("f>L n;cprod [5]", &[5.0]);
}

#[test]
fn cprod_fractional() {
    check_cprod("f>L n;cprod [2.0, 0.5, 3.0]", &[2.0, 1.0, 3.0]);
}

#[test]
fn cprod_with_zero() {
    // Once a zero appears, all subsequent products are also zero.
    check_cprod("f>L n;cprod [2,3,0,4]", &[2.0, 6.0, 0.0, 0.0]);
}

// ── interaction: prod + sum ───────────────────────────────────────────

#[test]
fn prod_sum_composed_cross_engine() {
    // Two helper-call numeric opcodes in one function body.
    let src = "f>n;+(prod [2, 3, 4]) (sum [1, 2, 3])";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, "f"), "30", "{engine}: prod + sum");
    }
}
