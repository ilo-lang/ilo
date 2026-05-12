// Cross-engine regression tests for the `cumsum` builtin.
//
// Signature: `cumsum xs:L n > L n`. Returns the running-sum list.
// Output length matches input length. Empty list → empty list.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_ok(engine: &str, src: &str) -> String {
    let out = ilo()
        .args([src, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
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

fn check_all(src: &str, expected: &[f64]) {
    for engine in ["--run-tree", "--run-vm"] {
        let got = parse_list(&run_ok(engine, src));
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
    #[cfg(feature = "cranelift")]
    {
        let got = parse_list(&run_ok("--run-cranelift", src));
        assert_eq!(
            got.len(),
            expected.len(),
            "engine=cranelift src=`{src}`: length mismatch"
        );
        for (i, (a, e)) in got.iter().zip(expected.iter()).enumerate() {
            assert!(
                (a - e).abs() < 1e-10,
                "engine=cranelift src=`{src}`: index {i} got {a}, expected {e}"
            );
        }
    }
}

#[test]
fn cumsum_basic() {
    check_all("f>L n;cumsum [1,2,3,4]", &[1.0, 3.0, 6.0, 10.0]);
}

#[test]
fn cumsum_empty() {
    check_all("f>L n;cumsum []", &[]);
}

#[test]
fn cumsum_singleton() {
    check_all("f>L n;cumsum [5]", &[5.0]);
}

#[test]
fn cumsum_alternating_signs() {
    check_all("f>L n;cumsum [1,-1,1,-1]", &[1.0, 0.0, 1.0, 0.0]);
}

#[test]
fn cumsum_fractional() {
    check_all("f>L n;cumsum [0.5, 0.25, 0.125]", &[0.5, 0.75, 0.875]);
}
