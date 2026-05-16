// Regression tests for prefix `??` (nil-coalesce).
//
// Previously `??` was infix-only: `c??0` worked but `??c 0` raised
// `ILO-P009 expected expression, got NilCoalesce`. Every other binary
// operator in ilo accepts a prefix form, so multiple personas tripped on
// the asymmetry. Prefix `??a b` now produces the same `Expr::NilCoalesce`
// shape as the infix form, and composes inside call args.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_file(engine: &str, src: &str, entry: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path =
        std::env::temp_dir().join(format!("ilo_prefix_nc_{}_{}.ilo", std::process::id(), seq));
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

const PREFIX_NIL: &str = "f>n;c=nil;??c 0";
const PREFIX_VAL: &str = "f>n;c=5;??c 0";
const INFIX_NIL: &str = "f>n;c=nil;c??0";
const INFIX_VAL: &str = "f>n;c=5;c??0";
const CALL_ARG_NIL: &str = "g x:n>n;x\nf>n;c=nil;g ??c 7\n";
const CALL_ARG_VAL: &str = "g x:n>n;x\nf>n;c=5;g ??c 7\n";
// Infix-default in prefix form: `?? c a+b` should accept a full expression
// as the default operand, matching infix `c??a+b`. Previously parse_operand
// would stop after parsing `a` and leave `+b` dangling.
const PREFIX_INFIX_DEFAULT_NIL: &str = "f>n;a=3;b=4;c=nil;?? c a+b";
const PREFIX_INFIX_DEFAULT_VAL: &str = "f>n;a=3;b=4;c=5;?? c a+b";
// Nested prefix nil-coalesce — confirm right-associativity matches infix.
const PREFIX_NESTED_NIL: &str = "f>n;c=nil;d=nil;??c ??d 0";
const PREFIX_NESTED_INNER: &str = "f>n;c=nil;d=9;??c ??d 0";
// Prefix `??` where the value side is a CALL expression. Previously the
// value side used `parse_operand` (atom-only), so `??mget m "k" 0` mis-parsed:
// `mget` was taken as the value atom and `m "k" 0` as the default expression
// (which then failed because `m` is a Map, not a function). With the arity-cap
// fix, the value side is parsed via `parse_call_arg`, so a known-arity
// function consumes exactly its declared arity and the remaining tokens
// become the default. Workarounds (`??(mget m "k") 0`, bind-first) keep
// working; the new shape is purely additive.
const PREFIX_CALL_HIT: &str = "f>n;m=mset mmap \"k\" 42;??mget m \"k\" 0";
const PREFIX_CALL_MISS: &str = "f>n;m=mset mmap \"k\" 42;??mget m \"missing\" 99";
// Chained prefix `??` with two call value-sides:
// `??mget m "x" ??mget m "a" 0` reads as `??(mget m "x") (??(mget m "a") 0)`,
// first miss, second hit, expect `1`.
const PREFIX_CALL_CHAIN: &str = "f>n;m=mset mmap \"a\" 1;??mget m \"x\" ??mget m \"a\" 0";
// Paren workaround still parses correctly post-fix.
const PREFIX_CALL_PAREN: &str = "f>n;m=mset mmap \"k\" 42;??(mget m \"k\") 0";

fn check_all(engine: &str) {
    assert_eq!(
        run(engine, PREFIX_NIL, "f"),
        "0",
        "prefix nil engine={engine}"
    );
    assert_eq!(
        run(engine, PREFIX_VAL, "f"),
        "5",
        "prefix val engine={engine}"
    );
    assert_eq!(
        run(engine, INFIX_NIL, "f"),
        "0",
        "infix nil engine={engine}"
    );
    assert_eq!(
        run(engine, INFIX_VAL, "f"),
        "5",
        "infix val engine={engine}"
    );
    assert_eq!(
        run_file(engine, CALL_ARG_NIL, "f"),
        "7",
        "call-arg nil engine={engine}"
    );
    assert_eq!(
        run_file(engine, CALL_ARG_VAL, "f"),
        "5",
        "call-arg val engine={engine}"
    );
    assert_eq!(
        run(engine, PREFIX_INFIX_DEFAULT_NIL, "f"),
        "7",
        "prefix infix-default nil engine={engine}"
    );
    assert_eq!(
        run(engine, PREFIX_INFIX_DEFAULT_VAL, "f"),
        "5",
        "prefix infix-default val engine={engine}"
    );
    assert_eq!(
        run(engine, PREFIX_NESTED_NIL, "f"),
        "0",
        "prefix nested nil engine={engine}"
    );
    assert_eq!(
        run(engine, PREFIX_NESTED_INNER, "f"),
        "9",
        "prefix nested inner engine={engine}"
    );
    assert_eq!(
        run(engine, PREFIX_CALL_HIT, "f"),
        "42",
        "prefix call hit engine={engine}"
    );
    assert_eq!(
        run(engine, PREFIX_CALL_MISS, "f"),
        "99",
        "prefix call miss engine={engine}"
    );
    assert_eq!(
        run(engine, PREFIX_CALL_CHAIN, "f"),
        "1",
        "prefix call chain engine={engine}"
    );
    assert_eq!(
        run(engine, PREFIX_CALL_PAREN, "f"),
        "42",
        "prefix call paren workaround engine={engine}"
    );
}

#[test]
fn prefix_nil_coalesce_tree() {
    check_all("--run-tree");
}

#[test]
fn prefix_nil_coalesce_vm() {
    check_all("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn prefix_nil_coalesce_cranelift() {
    check_all("--run-cranelift");
}
