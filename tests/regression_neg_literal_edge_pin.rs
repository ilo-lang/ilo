// Edge-case pinning tests for negative numeric literals.
//
// Companion to `regression_negative_literal_after_op.rs`, which pinned the
// natural shapes `<r -0.05`, `+a -3`, `mod n -2`, and `id -1.5`. These tests
// pin trickier edges that recent parser changes (PR #159 prefix-binop, PR #184
// fn-call-arg compose) could have shifted:
//
//   - `-0` literal evaluates to 0 (numeric, no NaN/sign weirdness leaks into
//     output) across tree/vm/cranelift.
//   - `[1 -2 3]` list literal: negative middle element parses cleanly and
//     `hd tl l` extracts -2.
//   - `- 0 0.05` explicit binary subtract with whitespace returns -0.05 (the
//     historical workaround for the lexer's eager `-` munch).
//   - `+ a -b` add of a param and a *negated identifier reference* (not a
//     negative literal): exercises the prefix-binop path where the second
//     operand is itself a unary-negate of a name, not a Number token.
//
// Every shape is run against all three engines.

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

fn run_file(engine: &str, src: &str, fn_name: &str, args: &[&str]) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "ilo_neg_edge_pin_{}_{}_{}.ilo",
        std::process::id(),
        seq,
        engine.trim_start_matches("--"),
    ));
    std::fs::write(&path, src).unwrap();
    let mut cmd = ilo();
    cmd.arg(path.to_str().unwrap()).arg(engine).arg(fn_name);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for file src=`{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// `-0` literal returns 0.
const NEG_ZERO: &str = "f>n;-0";
// list with negative middle element: `[1 -2 3]`; extract middle via `hd tl l`.
const LIST_NEG_MID: &str = "f>n;l=[1 -2 3];hd tl l";
// explicit binary subtract with whitespace: `- 0 0.05` => -0.05.
const SUB_SPACE: &str = "f>n;- 0 0.05";
// prefix-add where second operand is a negated identifier ref (not a literal).
const ADD_NEG_REF: &str = "f a:n b:n>n;+a -b";

fn check_all(engine: &str) {
    // 1. `-0` literal.
    assert_eq!(
        run(engine, NEG_ZERO, &["f"]),
        "0",
        "neg-zero engine={engine}"
    );

    // 2. list literal `[1 -2 3]` middle element.
    assert_eq!(
        run_file(engine, LIST_NEG_MID, "f", &[]),
        "-2",
        "list-neg-mid engine={engine}"
    );

    // 3. `- 0 0.05` explicit binary subtract returns -0.05.
    assert_eq!(
        run_file(engine, SUB_SPACE, "f", &[]),
        "-0.05",
        "sub-space engine={engine}"
    );

    // 4. `+ a -b` with a=10, b=3 returns 7 (10 + -3); with a=-5, b=3 returns -8.
    assert_eq!(
        run(engine, ADD_NEG_REF, &["f", "10", "3"]),
        "7",
        "add-neg-ref 10 + -3 engine={engine}"
    );
    assert_eq!(
        run(engine, ADD_NEG_REF, &["f", "-5", "3"]),
        "-8",
        "add-neg-ref -5 + -3 engine={engine}"
    );
    assert_eq!(
        run(engine, ADD_NEG_REF, &["f", "0", "0"]),
        "0",
        "add-neg-ref 0 + -0 engine={engine}"
    );
}

#[test]
fn neg_literal_edge_pin_tree() {
    check_all("--run-tree");
}

#[test]
fn neg_literal_edge_pin_vm() {
    check_all("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn neg_literal_edge_pin_cranelift() {
    check_all("--run-cranelift");
}
