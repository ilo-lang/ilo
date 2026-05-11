// Regression tests for negative numeric literals appearing immediately after
// a binary operator or in a call/prefix-binop argument slot.
//
// The lexer eats a leading `-` as part of the Number token, so `<r -0.05`
// produces tokens `Less, Ident(r), Number(-0.05)` rather than three tokens
// for binary subtract. Multiple personas in real-world runs reported the
// natural form `<r -0.05` parsing badly and had to rewrite it as
// `nt=- 0 0.05; <r nt`. These tests pin the natural form across all engines:
// a negative literal is itself a valid operand to a binary op, a prefix-binop
// arg, and a function call arg.
//
// Variants exercised:
//   - `<r -0.05{1}{0}` ternary against a negative threshold
//   - `+a -3` prefix-binary add with a negative literal second operand
//   - `mod n -2` builtin call with a negative literal arg
//   - `id -1.5` ordinary function call with a negative literal arg

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}` args={args:?}: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

const BELOW: &str = "f r:n>n;<r -0.05{1}{0}";
const PREFIX_ADD: &str = "f a:n>n;+a -3";
const MOD_NEG: &str = "f n:n>n;mod n -2";
const ID_CALL: &str = "id x:n>n;x\nf>n;id -1.5\n";

fn check_all(engine: &str) {
    // `<r -0.05` ternary: r below the threshold returns 1, otherwise 0.
    assert_eq!(
        run(engine, BELOW, &["f", "-0.1"]),
        "1",
        "below: r=-0.1 engine={engine}"
    );
    assert_eq!(
        run(engine, BELOW, &["f", "0.1"]),
        "0",
        "below: r=0.1 engine={engine}"
    );
    assert_eq!(
        run(engine, BELOW, &["f", "-0.05"]),
        "0",
        "below: r=-0.05 (boundary, not strictly less) engine={engine}"
    );

    // Prefix-binary add with a negative literal second operand.
    assert_eq!(
        run(engine, PREFIX_ADD, &["f", "10"]),
        "7",
        "prefix-add: 10 + -3 engine={engine}"
    );
    assert_eq!(
        run(engine, PREFIX_ADD, &["f", "-5"]),
        "-8",
        "prefix-add: -5 + -3 engine={engine}"
    );

    // Builtin call with a negative literal arg.
    assert_eq!(
        run(engine, MOD_NEG, &["f", "7"]),
        "1",
        "mod: 7 mod -2 engine={engine}"
    );
    assert_eq!(
        run(engine, MOD_NEG, &["f", "10"]),
        "0",
        "mod: 10 mod -2 engine={engine}"
    );
}

fn check_id(engine: &str) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "ilo_neg_after_op_{}_{}_{}.ilo",
        std::process::id(),
        seq,
        engine.trim_start_matches("--"),
    ));
    std::fs::write(&path, ID_CALL).unwrap();
    let out = ilo()
        .args([path.to_str().unwrap(), engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for id-call: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "-1.5",
        "id-call: -1.5 round-trip engine={engine}"
    );
}

#[test]
fn neg_after_op_tree() {
    check_all("--run-tree");
    check_id("--run-tree");
}

#[test]
fn neg_after_op_vm() {
    check_all("--run-vm");
    check_id("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn neg_after_op_cranelift() {
    check_all("--run-cranelift");
    check_id("--run-cranelift");
}
