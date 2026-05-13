// Regression tests for the `-0 v` papercut: a glued negative-literal token
// at a fresh-expression position (start of input, after `;`, after `=`, after
// `{`, after `(`) silently produced wrong results because Logos's
// `-?[0-9]+...` regex consumed the leading `-`. So `ab x:n>n;-0 x` lexed as
// `Number(-0)` followed by a stray `Ref(x)` rather than the user's intended
// `Subtract(0, x)`. Six+ personas hit this in the assessment log writing
// numerical formulas where "0 minus v" is the natural unary-negation idiom.
//
// The lexer now splits `Number(-N)` into `Minus, Number(N)` when the
// preceding token is one that introduces a fresh expression position. Call-
// arg negative literals (`at xs -1`, `+a -3`, `into -3 0 10`, `[1 -2 3]`,
// `[-2 1 3]`) are deliberately preserved by *excluding* value-producing
// tokens and `LBracket` from the split contexts.
//
// These tests pin the fixed behaviour across all three engines.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, args: &[&str]) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "ilo_neg_papercut_{}_{}_{}.ilo",
        std::process::id(),
        seq,
        engine.trim_start_matches("--"),
    ));
    std::fs::write(&path, src).unwrap();
    let mut cmd = ilo();
    cmd.arg(path.to_str().unwrap()).arg(engine);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for src=`{src}` args={args:?}: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// Unary-negate-by-subtract-from-zero at statement position. This is the
// dominant form of the papercut.
const AB_AT_START: &str = "ab x:n>n;-0 x\n";

// Same shape inside a function body after a `;` separator (`v=...;-0 v`).
const ABSP_AFTER_SEMI: &str = "absp p:L _>n;v=p.1;-0 v\n";

// Negative-literal subtract on the rhs of an assignment (`r=-1 t2`).
const DIFF_AFTER_EQ: &str = "diff a:n b:n>n;r=-a b;r\n";

// `-0 c` inside a braced block (function body / conditional arm).
const ZARM_INSIDE_BRACE: &str = "zarm c:n>n;<c 0{-0 c}{c}\n";

// `-0 n` inside a parenthesised expression (safe-ending wrap).
const NEG_INSIDE_PAREN: &str = "neg n:n>n;(-0 n)\n";

// Pin keep-literal cases that MUST NOT split, even after the fix:
//
//   - `[1 -2 3]`: middle element follows a Number, stays negative literal.
//   - `[-2 1 3]`: first element follows `LBracket` (deliberately excluded
//     from the split contexts), stays negative literal. If LBracket *did*
//     split, the parser would greedy-subtract `-2 1` and silently produce
//     a 2-element list `[1, 3]`.
const MID_LIST: &str = "mid>n;l=[1 -2 3];hd tl l\n";
const FIRST_NEG: &str = "first>n;l=[-2 1 3];hd l\n";
const LIST_LEN: &str = "ln>n;l=[-2 1 3];len l\n";

fn check_engine(engine: &str) {
    // -0 x at fresh-expression position correctly negates.
    assert_eq!(
        run(engine, AB_AT_START, &["ab", "7"]),
        "-7",
        "ab(7) engine={engine}"
    );
    assert_eq!(
        run(engine, AB_AT_START, &["ab", "-7"]),
        "7",
        "ab(-7) engine={engine}"
    );
    assert_eq!(
        run(engine, AB_AT_START, &["ab", "0"]),
        "0",
        "ab(0) engine={engine}"
    );

    // -0 v after a `;` (mid-function-body) reads the list element through
    // a prior binding and negates it.
    assert_eq!(
        run(engine, ABSP_AFTER_SEMI, &["absp", "[10,3]"]),
        "-3",
        "absp engine={engine}"
    );

    // r=-a b on the rhs of an assignment is `Subtract(a, b)`.
    assert_eq!(
        run(engine, DIFF_AFTER_EQ, &["diff", "5", "2"]),
        "3",
        "diff(5,2) engine={engine}"
    );
    assert_eq!(
        run(engine, DIFF_AFTER_EQ, &["diff", "2", "5"]),
        "-3",
        "diff(2,5) engine={engine}"
    );

    // {-0 c} inside the then-arm of a guard returns the absolute value.
    assert_eq!(
        run(engine, ZARM_INSIDE_BRACE, &["zarm", "-4"]),
        "4",
        "zarm(-4) engine={engine}"
    );
    assert_eq!(
        run(engine, ZARM_INSIDE_BRACE, &["zarm", "6"]),
        "6",
        "zarm(6) engine={engine}"
    );

    // (-0 n) inside parens negates.
    assert_eq!(
        run(engine, NEG_INSIDE_PAREN, &["neg", "9"]),
        "-9",
        "neg(9) engine={engine}"
    );
    assert_eq!(
        run(engine, NEG_INSIDE_PAREN, &["neg", "-2"]),
        "2",
        "neg(-2) engine={engine}"
    );

    // Keep-literal: `[1 -2 3]` is a 3-element list; `hd tl l` returns the
    // second element (-2).
    assert_eq!(
        run(engine, MID_LIST, &["mid"]),
        "-2",
        "mid list `[1 -2 3]` engine={engine}"
    );

    // Keep-literal: `[-2 1 3]` is a 3-element list starting with -2.
    assert_eq!(
        run(engine, FIRST_NEG, &["first"]),
        "-2",
        "first of `[-2 1 3]` engine={engine}"
    );
    assert_eq!(
        run(engine, LIST_LEN, &["ln"]),
        "3",
        "len of `[-2 1 3]` engine={engine}"
    );
}

#[test]
fn neg_literal_papercut_tree() {
    check_engine("--run-tree");
}

#[test]
fn neg_literal_papercut_vm() {
    check_engine("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn neg_literal_papercut_cranelift() {
    check_engine("--run-cranelift");
}
