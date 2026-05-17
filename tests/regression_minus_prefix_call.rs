// Regression tests for prefix-minus where an operand is a known-arity call.
//
// Companion to `regression_prefix_binop_call.rs` (#332). That PR routed
// `parse_prefix_binop` operands through `parse_prefix_binop_operand` so
// `>len q 0` expands the `len q` call before the comparison. The `-`
// family was missed because `parse_minus` is its own arm (handles unary
// vs binary minus disambiguation via `can_start_operand`), so it kept
// calling `parse_operand` directly and `-lnx a lnx b` mis-parsed as
// `BinOp(-, Ref(lnx), Ref(a))` with `lnx b` orphaned. Probe from the
// scientific-researcher persona (#266 carry-forward):
//
//   main>n;-lnx 5 lnx 3
//
// failed with `ILO-P003 expected Greater, got Number(3.0)` because the
// orphaned tail `lnx 3` fell out into statement position.
//
// Fix wires `parse_prefix_binop_operand` into `parse_minus`. Unary
// negation of a call (`-lnx 5`) still parses correctly — the helper
// consumes the call as one operand, `can_start_operand` returns false,
// and we fall into the Negate arm with the call inside.
//
// Cross-engine: tree, VM, Cranelift. Parse is shared but every backend
// codegens the resulting `Negate(Call(...))` and `BinOp(-, Call, Call)`
// nodes, so exercise all three.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "ilo_minus_prefix_call_{}_{}.ilo",
        std::process::id(),
        seq
    ));
    std::fs::write(&path, src).unwrap();
    let out = ilo()
        .args([path.to_str().unwrap(), engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// Originating shape from scientific-researcher (sub two calls). The
// probe used `lnx` but the parser failure is identical for any 1-arg
// user fn, so test with `dbl x = x*2` for exact cross-engine integer
// answers: `-dbl 5 dbl 3` = 10 - 6 = 4.
const MINUS_BOTH_CALLS: &str = "dbl>x:n>n\n;ret *x 2\nmain>n\n;-dbl 5 dbl 3";

// Unary negation of a call: `-dbl 5` should be `-(dbl 5)` = -10. This
// is the disambiguation test — `parse_prefix_binop_operand` consumes
// `dbl 5` as one call, `can_start_operand` is false, Negate fires.
const NEGATE_CALL: &str = "dbl>x:n>n\n;ret *x 2\nmain>n\n;- dbl 5";

// Left operand is a call, right is a literal: `-dbl 5 3` = 10 - 3 = 7.
const MINUS_LEFT_CALL: &str = "dbl>x:n>n\n;ret *x 2\nmain>n\n;-dbl 5 3";

// Left operand is a literal, right is a call: `-10 dbl 3` = 10 - 6 = 4.
const MINUS_RIGHT_CALL: &str = "dbl>x:n>n\n;ret *x 2\nmain>n\n;-10 dbl 3";

// Negative regression: bare locals on both sides must keep working.
// `a=5;b=3;-a b` = 2. No fn_arity entry for `a`/`b`, falls through to
// `parse_operand`.
const MINUS_BARE_LOCALS: &str = "main>n\n;a=5\n;b=3\n;-a b";

// Negative regression: unary negate of a bare local. `a=5;-a` = -5.
const NEGATE_BARE_LOCAL: &str = "main>n\n;a=5\n;- a";

// `*` mixed with `-`: prove the call-arg expansion plays nicely when
// nested inside a `*` left operand. `*-dbl 5 3 2` should parse as
// `(dbl 5 - 3) * 2` = 7 * 2 = 14 via the prefix arithmetic chain.
const MINUS_INSIDE_STAR: &str = "dbl>x:n>n\n;ret *x 2\nmain>n\n;*-dbl 5 3 2";

fn check_all(engine: &str) {
    assert_eq!(
        run(engine, MINUS_BOTH_CALLS, "main"),
        "4",
        "-dbl 5 dbl 3 engine={engine}"
    );
    assert_eq!(
        run(engine, NEGATE_CALL, "main"),
        "-10",
        "- dbl 5 engine={engine}"
    );
    assert_eq!(
        run(engine, MINUS_LEFT_CALL, "main"),
        "7",
        "-dbl 5 3 engine={engine}"
    );
    assert_eq!(
        run(engine, MINUS_RIGHT_CALL, "main"),
        "4",
        "-10 dbl 3 engine={engine}"
    );
    assert_eq!(
        run(engine, MINUS_BARE_LOCALS, "main"),
        "2",
        "-a b bare locals engine={engine}"
    );
    assert_eq!(
        run(engine, NEGATE_BARE_LOCAL, "main"),
        "-5",
        "- a bare local engine={engine}"
    );
    assert_eq!(
        run(engine, MINUS_INSIDE_STAR, "main"),
        "14",
        "*-dbl 5 3 2 engine={engine}"
    );
}

#[test]
fn minus_prefix_call_tree() {
    check_all("--run-tree");
}

#[test]
fn minus_prefix_call_vm() {
    check_all("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn minus_prefix_call_cranelift() {
    check_all("--run-cranelift");
}
