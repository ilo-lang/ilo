// Regression tests for prefix-binop where an operand is a known-arity call.
//
// Previously `parse_prefix_binop` parsed both operands via `parse_operand`
// (atom-only), so `>len q 0` mis-parsed as `BinOp(>, Ref(len), Ref(q))` with
// `0` left orphaned. This was a 6-rerun P1 standing item across personas —
// `wh` header, guard, let-RHS, and prefix-ternary all failed on first
// attempt when the prefix-binop operand was a call expression, forcing
// every persona to retry with bind-first or parens.
//
// Fix mirrors the prefix-`??` precedent (#310): swap `parse_operand` →
// `parse_call_arg(false, None)` for both operands so a known-arity ident
// expands into a call expression, consuming exactly its declared arity.
// Bare locals (no fn_arity entry) fall through to `parse_operand`
// unchanged, so the historical `wh >v 0` shape keeps working.
//
// Cross-engine: parses are pure parser-side so tree/VM/Cranelift all see
// the same AST, but we exercise every backend anyway to catch any
// downstream codegen surprise from the new call nodes.
//
// Note: `+f g` where `f` is a 1-arity user-defined fn now parses as
// `BinOp(+, Call(f, [g]), <next>)` instead of `BinOp(+, Ref(f), Ref(g))`,
// matching the manifesto-aligned arity-cap behaviour. The bare-ref shape
// was never meaningful (you can't add two function references) so this
// is a clean upgrade. Consistent with the same trade `??` makes.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "ilo_prefix_binop_call_{}_{}.ilo",
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

// The originating shape from gis-analyst: `wh >len q 0{...}`. Tail-shrinks
// a list with a prefix-call condition. Pre-fix this errored
// `ILO-P003 expected LBrace, got Number(0.0)`; post-fix it parses cleanly
// and runs to completion (q drained, len reaches 0).
const WH_HEAD_LEN_GT: &str = "main>n;q=[1,2,3];wh >len q 0{q=tl q};len q";

// Same family with `<` (less-than) — `wh <len q 3` shrinks while the
// list is shorter than 3 elements (always false here, body never runs).
const WH_HEAD_LEN_LT: &str = "main>n;q=[1,2,3];wh <len q 3{q=tl q};len q";

// `<i (len xs)` — call wrapped in parens on the RIGHT operand. Should
// already have worked (parens are a separate code path) but pin it as a
// no-regression check.
const WH_HEAD_PAREN_RIGHT: &str = "main>n;xs=[10,20,30];i=0;s=0;wh <i (len xs){s=+s xs.i;i=+i 1};s";

// Guard with prefix-call condition: `=>len q 1{...}` should parse the
// `>` as a comparison (not a return-type arrow). Tests the guard path
// since it routes through `parse_expr_or_guard` not the `wh` arm.
const GUARD_PREFIX_CALL: &str = "main>n;q=[1,2,3];>len q 1{ret 42};0";

// Assignment RHS: `v = >len q 0` binds a bool. Tests `parse_let` path.
const LET_RHS_PREFIX_CALL: &str = "main>n;q=[1,2,3];v=>len q 0;?v{1}{0}";

// Prefix-ternary on prefix-call: `?>len q 0 a b`. The prefix-ternary
// helper parses the condition via `parse_prefix_binop`, so the fix here
// flows through automatically.
const PREFIX_TERNARY_CALL: &str = "main>n;q=[1,2,3];?>len q 0 100 0";
const PREFIX_TERNARY_CALL_EMPTY: &str = "main>n;q=[];?>len q 0 100 0";

// Negative regression: `wh >v 0` with a bare local `v` (no fn_arity
// entry) must still work — falls through to `parse_operand` unchanged.
// Exact shape from the historic `examples/wh-gt-condition.ilo`.
const WH_BARE_LOCAL: &str = "main>n;v=3;wh >v 0{v=- v 1};v";

// Builtin in left-operand slot via prefix `=`: `==len xs 3` (equality
// against a call). Different from `>`/`<` since `=` is the comparison
// op. Confirms the fix isn't `>`-specific.
const WH_EQ_LEN: &str = "main>n;xs=[1,2];i=0;wh ==len xs 2{i=+i 1;xs=tl xs};i";

// Both operands as calls: `>len xs len ys`. Each `len` is arity-1 so
// `parse_call_arg` consumes `len xs` then `len ys`, leaving nothing
// orphaned. Pin this in case anyone later assumes only left is a call.
const WH_BOTH_CALLS: &str = "main>n;xs=[1,2,3];ys=[1,2];n=0;wh >len xs len ys{xs=tl xs;n=+n 1};n";

fn check_all(engine: &str) {
    assert_eq!(
        run(engine, WH_HEAD_LEN_GT, "main"),
        "0",
        "wh >len q 0 engine={engine}"
    );
    assert_eq!(
        run(engine, WH_HEAD_LEN_LT, "main"),
        "3",
        "wh <len q 3 engine={engine}"
    );
    assert_eq!(
        run(engine, WH_HEAD_PAREN_RIGHT, "main"),
        "60",
        "wh <i (len xs) engine={engine}"
    );
    assert_eq!(
        run(engine, GUARD_PREFIX_CALL, "main"),
        "42",
        "guard >len q 1 engine={engine}"
    );
    assert_eq!(
        run(engine, LET_RHS_PREFIX_CALL, "main"),
        "1",
        "let RHS >len q 0 engine={engine}"
    );
    assert_eq!(
        run(engine, PREFIX_TERNARY_CALL, "main"),
        "100",
        "prefix-ternary >len q 0 engine={engine}"
    );
    assert_eq!(
        run(engine, PREFIX_TERNARY_CALL_EMPTY, "main"),
        "0",
        "prefix-ternary >len []=0 engine={engine}"
    );
    assert_eq!(
        run(engine, WH_BARE_LOCAL, "main"),
        "0",
        "wh >v 0 bare local engine={engine}"
    );
    assert_eq!(
        run(engine, WH_EQ_LEN, "main"),
        "1",
        "wh ==len xs 2 engine={engine}"
    );
    assert_eq!(
        run(engine, WH_BOTH_CALLS, "main"),
        "1",
        "wh >len xs len ys engine={engine}"
    );
}

#[test]
fn prefix_binop_call_tree() {
    check_all("--run-tree");
}

#[test]
fn prefix_binop_call_vm() {
    check_all("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn prefix_binop_call_cranelift() {
    check_all("--run-cranelift");
}
