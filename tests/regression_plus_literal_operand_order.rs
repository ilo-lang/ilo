// Regression tests: commutative prefix operators accept literal-first or
// ref-first operands symmetrically.
//
// quant-trader rerun9 (assessment doc) reported `onep=+ 1 contrib` failing
// to parse while `onep=+ contrib 1` worked. On current main both forms
// parse cleanly across every engine and produce identical
// `BinOp Add(Literal, Ref)` ASTs - the bug appears to have been fixed by
// an earlier change that landed before 0.11.8 was tagged. This file locks
// the symmetry contract so any future regression of operand-order parity
// trips CI rather than the next persona run.
//
// Covers `+`, `*`, `&`, `|` - the four commutative prefix operators where
// the user-facing claim is that operand order doesn't matter. The
// matching example file `examples/plus-literal-operand-order.@` exercises
// the same shapes via the `tests/examples_engines.rs` harness; this file
// adds the directly-asserted forms the persona reported plus harder
// shapes (let-RHS, foreach body, ternary RHS) that the example format
// can't easily express.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str, args: &[&str]) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path =
        std::env::temp_dir().join(format!("ilo_plus_literal_{}_{}.@", std::process::id(), seq));
    std::fs::write(&path, src).unwrap();
    let mut cmd_args: Vec<&str> = vec![path.to_str().unwrap(), engine, entry];
    cmd_args.extend_from_slice(args);
    let out = ilo().args(&cmd_args).output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// The originating shape: `onep=+ 1 contrib` as a let-RHS.
const LET_PLUS_LIT_FIRST: &str = "main contrib:n>n;onep=+ 1 contrib;onep";
const LET_PLUS_REF_FIRST: &str = "main contrib:n>n;onep=+ contrib 1;onep";

// Same in `*` (other arithmetic commutative).
const LET_MUL_LIT_FIRST: &str = "main x:n>n;y=* 3 x;y";
const LET_MUL_REF_FIRST: &str = "main x:n>n;y=* x 3;y";

// Boolean commutatives. `&` and `|` are the prefix logical ops.
const LET_AND_LIT_FIRST: &str = "main b:b>b;r=& true b;r";
const LET_AND_REF_FIRST: &str = "main b:b>b;r=& b true;r";
const LET_OR_LIT_FIRST: &str = "main b:b>b;r=| false b;r";
const LET_OR_REF_FIRST: &str = "main b:b>b;r=| b false;r";

// Inside a foreach body - mirrors the quant-trader rerun9 site exactly
// (`@k 0..nsm{...; onep=+ contrib 1; ...}`). The literal-first variant
// must parse just as cleanly when sitting between other statements.
const FOREACH_PLUS_LIT_FIRST: &str =
    "main n:n>n;s=0;@i 0..n{contrib=*i 2;onep=+ 1 contrib;s=+ s onep};s";
const FOREACH_PLUS_REF_FIRST: &str =
    "main n:n>n;s=0;@i 0..n{contrib=*i 2;onep=+ contrib 1;s=+ s onep};s";

// As a direct return expression (no binding). The parser routes this
// through `parse_operand` rather than `parse_let`, so it exercises a
// slightly different path.
const RET_PLUS_LIT_FIRST: &str = "main x:n>n;+ 1 x";
const RET_PLUS_REF_FIRST: &str = "main x:n>n;+ x 1";

// Nested inside another binop: `*2 + 1 x` = `2 * (1 + x)`. The inner `+`
// is in operand position of the outer `*`, so this exercises operand
// parsing one level deep.
const NESTED_PLUS_LIT_FIRST: &str = "main x:n>n;*2 + 1 x";
const NESTED_PLUS_REF_FIRST: &str = "main x:n>n;*2 + x 1";

// Prefix-ternary operand slot: `?>x 0 + 1 x + x 1`. Both then-arm and
// else-arm use literal-first `+` so the value coming out doesn't change,
// but parser must consume `+ 1 x` as a single operand expression each
// time (not greedy across to the else arm).
const TERNARY_PLUS_LIT_FIRST: &str = "main x:n>n;?>x 0 (+ 1 x) (+ 1 x)";
const TERNARY_PLUS_REF_FIRST: &str = "main x:n>n;?>x 0 (+ x 1) (+ x 1)";

fn check_all(engine: &str) {
    // The originating let-RHS shape.
    assert_eq!(
        run(engine, LET_PLUS_LIT_FIRST, "main", &["5"]),
        "6",
        "let onep=+ 1 contrib engine={engine}"
    );
    assert_eq!(
        run(engine, LET_PLUS_REF_FIRST, "main", &["5"]),
        "6",
        "let onep=+ contrib 1 engine={engine}"
    );

    // `*` parity.
    assert_eq!(
        run(engine, LET_MUL_LIT_FIRST, "main", &["7"]),
        "21",
        "let y=* 3 x engine={engine}"
    );
    assert_eq!(
        run(engine, LET_MUL_REF_FIRST, "main", &["7"]),
        "21",
        "let y=* x 3 engine={engine}"
    );

    // `&` parity.
    assert_eq!(
        run(engine, LET_AND_LIT_FIRST, "main", &["true"]),
        "true",
        "let r=& true b (b=true) engine={engine}"
    );
    assert_eq!(
        run(engine, LET_AND_REF_FIRST, "main", &["true"]),
        "true",
        "let r=& b true (b=true) engine={engine}"
    );
    assert_eq!(
        run(engine, LET_AND_LIT_FIRST, "main", &["false"]),
        "false",
        "let r=& true b (b=false) engine={engine}"
    );
    assert_eq!(
        run(engine, LET_AND_REF_FIRST, "main", &["false"]),
        "false",
        "let r=& b true (b=false) engine={engine}"
    );

    // `|` parity.
    assert_eq!(
        run(engine, LET_OR_LIT_FIRST, "main", &["false"]),
        "false",
        "let r=| false b (b=false) engine={engine}"
    );
    assert_eq!(
        run(engine, LET_OR_REF_FIRST, "main", &["false"]),
        "false",
        "let r=| b false (b=false) engine={engine}"
    );
    assert_eq!(
        run(engine, LET_OR_LIT_FIRST, "main", &["true"]),
        "true",
        "let r=| false b (b=true) engine={engine}"
    );
    assert_eq!(
        run(engine, LET_OR_REF_FIRST, "main", &["true"]),
        "true",
        "let r=| b false (b=true) engine={engine}"
    );

    // Foreach-body site - exact shape from quant-trader rerun9.
    // n=3: contrib in {0,2,4}; onep in {1,3,5}; s = 1+3+5 = 9.
    assert_eq!(
        run(engine, FOREACH_PLUS_LIT_FIRST, "main", &["3"]),
        "9",
        "foreach onep=+ 1 contrib engine={engine}"
    );
    assert_eq!(
        run(engine, FOREACH_PLUS_REF_FIRST, "main", &["3"]),
        "9",
        "foreach onep=+ contrib 1 engine={engine}"
    );

    // Return expression.
    assert_eq!(
        run(engine, RET_PLUS_LIT_FIRST, "main", &["5"]),
        "6",
        "ret + 1 x engine={engine}"
    );
    assert_eq!(
        run(engine, RET_PLUS_REF_FIRST, "main", &["5"]),
        "6",
        "ret + x 1 engine={engine}"
    );

    // Nested: 2 * (1 + 5) = 12.
    assert_eq!(
        run(engine, NESTED_PLUS_LIT_FIRST, "main", &["5"]),
        "12",
        "nested *2 + 1 x engine={engine}"
    );
    assert_eq!(
        run(engine, NESTED_PLUS_REF_FIRST, "main", &["5"]),
        "12",
        "nested *2 + x 1 engine={engine}"
    );

    // Prefix-ternary: x=5, x>0 true, then-arm 1+5=6.
    assert_eq!(
        run(engine, TERNARY_PLUS_LIT_FIRST, "main", &["5"]),
        "6",
        "ternary +1 x in then engine={engine}"
    );
    assert_eq!(
        run(engine, TERNARY_PLUS_REF_FIRST, "main", &["5"]),
        "6",
        "ternary +x 1 in then engine={engine}"
    );
}

#[test]
fn plus_literal_operand_order_tree() {
    check_all("--vm");
}

#[test]
fn plus_literal_operand_order_vm() {
    check_all("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn plus_literal_operand_order_cranelift() {
    check_all("--jit");
}
