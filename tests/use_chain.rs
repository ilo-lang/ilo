// Tests for the use-chain `x <- expr` desugaring (ILO-409).
//
// `x <- expr ; rest` desugars to `?expr{~x: rest; ^e: ^e}`, which flattens
// multi-step R-T-E unwrap arms into a flat sequential style.
//
// Single step:
//   x <- parse s
//   +x 1
// desugars to:
//   ?parse s{~x: +x 1; ^e: ^e}
//
// Two steps:
//   x <- parse s
//   y <- num str-val
//   +x y
// desugars to:
//   ?parse s{~x: ?num str-val{~y: +x y; ^e: ^e}; ^e: ^e}

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_vm(src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, "--vm", entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo --vm failed: stderr={}\nsrc={src}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// --- single use-chain step ---

// `x <- num "42" ; +x 1` desugars to `?num "42"{~x: +x 1; ^e: ^e}` → 43
const SINGLE_STEP_OK: &str = r#"f>n;x <- num "42";+x 1"#;

#[test]
fn single_step_ok() {
    assert_eq!(run_vm(SINGLE_STEP_OK, "f"), "43");
}

// When the RTE fails, the err is propagated out of the enclosing R-returning fn.
// `x <- num "bad!"` desugars the error arm to `^e: ^e`, which propagates.
// We use "bad!" so it's clearly a text error value (not the math constant `e`).
#[test]
fn single_step_err_propagated() {
    // inner returns R n t; use-chain propagates the num parse error.
    // g wraps the call and checks the error arm ran (returns sentinel "err").
    const SRC: &str = r#"inner>R n t;x <- num "not-a-number";~(+x 1)
g>t;r=inner;?r{~v:str v;^e:"err"}"#;
    assert_eq!(run_vm(SRC, "g"), "err");
}

// --- two chained use-chain steps ---

// `x <- num "10" ; y <- num "5" ; +x y` → 15
const TWO_STEP_OK: &str = r#"f>n;x <- num "10";y <- num "5";+x y"#;

#[test]
fn two_step_ok() {
    assert_eq!(run_vm(TWO_STEP_OK, "f"), "15");
}

// Error in first step propagates; second step never runs.
// outer: x <- num "not-a-number" fails, y step never executes.
// g detects the error arm ran (returns "first-err").
#[test]
fn two_step_first_err_propagated() {
    const SRC: &str = r#"outer>R n t;x <- num "not-a-number";y <- num "5";~(+x y)
g>t;r=outer;?r{~v:str v;^e:"first-err"}"#;
    assert_eq!(run_vm(SRC, "g"), "first-err");
}

// Error in second step propagates; first step succeeds.
// outer: x=10 ok, y <- num "not-a-number" fails.
// g detects the error arm ran (returns "second-err").
#[test]
fn two_step_second_err_propagated() {
    const SRC: &str = r#"outer>R n t;x <- num "10";y <- num "not-a-number";~(+x y)
g>t;r=outer;?r{~v:str v;^e:"second-err"}"#;
    assert_eq!(run_vm(SRC, "g"), "second-err");
}

// --- three chained steps (flat chain) ---

const THREE_STEP_OK: &str = r#"f>n;x <- num "2";y <- num "3";z <- num "4";*x (+y z)"#;

#[test]
fn three_step_ok() {
    // 2 * (3 + 4) = 14
    assert_eq!(run_vm(THREE_STEP_OK, "f"), "14");
}

// --- use-chain with intervening plain statements ---

// Plain lets between use-chain steps still work
const WITH_PLAIN_LETS: &str = r#"f>n;x <- num "10";doubled=*x 2;y <- num "3";+doubled y"#;

#[test]
fn with_plain_lets() {
    // doubled = 20, y = 3, 20 + 3 = 23
    assert_eq!(run_vm(WITH_PLAIN_LETS, "f"), "23");
}
