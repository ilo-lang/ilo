// Regression tests for greedy call-expansion of known functions inside list
// literals. Companion to `regression_list_literal_refs.rs`.
//
// Before this change, `cat ["proteins=" str n] ""` failed with ILO-R009
// (`cat: list items must be text, got FnRef("str")`) because the parser
// treated every Ident inside a whitespace list-literal as a bare ref. That
// forced agents to bind every formatted value first (`sn=str n; cat
// ["proteins=" sn] ""`), which burns tokens on every retry and surfaced
// 6+ times in a single 100-LOC bioinformatics program.
//
// Refined rule: inside a list literal in whitespace mode, an Ident that is
// a known function (present in `fn_arity` with arity > 0) followed by an
// operand parses as a call with EXACTLY arity operands consumed, mirroring
// the nested-call rule in `parse_call_arg`. Idents NOT in `fn_arity` (e.g.
// local variables) stay as bare elements, so `[a b c]` with `a=1;b=2;c=3`
// still yields a 3-element list. HOF fn-ref positions (`map dbl xs`,
// `srt cmp xs`) keep their inner ref because `parse_call_arg` respects
// `is_fn_ref_position`.
//
// The arity cap is critical: without it, `[at xs 0 at xs 2]` would parse
// as `at(xs, 0, at, xs, 2)` (5 args) instead of two element calls.
//
// Cross-engine: tree, vm, and (when enabled) cranelift JIT — the parser
// change is shared so each engine must see the same AST and emit the same
// output.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(name: &str, src: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "ilo_listlit_fnref_{name}_{}_{n}.ilo",
        std::process::id()
    ));
    std::fs::write(&path, src).expect("write src");
    path
}

fn run_ok(engine: &str, src: &str, entry: &str, args: &[&str]) -> String {
    let path = write_src(entry, src);
    let mut cmd = ilo();
    cmd.arg(&path).arg(engine).arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// --- Tier 1: original repro - the exact friction the user hit ----------

// `cat ["proteins=" str n] ""` previously failed with ILO-R009 because
// `str` was kept as a bare Ref → FnRef and cat barfed. With the fix,
// `str n` greedy-parses inside the list literal as Call(str, [n]) and the
// cat join produces the expected text.
const REPRO_PROTEIN: &str = "f n:n>t;cat [\"proteins=\" str n] \"\"";

fn check_repro(engine: &str) {
    assert_eq!(
        run_ok(engine, REPRO_PROTEIN, "f", &["42"]),
        "proteins=42",
        "original cat-with-str repro now works {engine}",
    );
}

// --- Tier 2: 1-arg builtins as list elements ----------------------------

const STR_OF_LOCAL: &str = "f>L t;n=42;[str n]";
const LEN_OF_LIST: &str = "f>L n;xs=[1 2 3];[len xs]";

fn check_unary(engine: &str) {
    assert_eq!(
        run_ok(engine, STR_OF_LOCAL, "f", &[]),
        "[42]",
        "str eats one operand in list literal {engine}",
    );
    assert_eq!(
        run_ok(engine, LEN_OF_LIST, "f", &[]),
        "[3]",
        "len eats one operand in list literal {engine}",
    );
}

// --- Tier 3: 2-arg builtins (arity-capped greedy) -----------------------

// `at` is binary. Without the arity cap, `[at xs 0 at xs 2]` would parse
// as `[Call(at, [xs, 0, at, xs, 2])]` and fail with arity-mismatch. With
// the cap, each `at` eats exactly 2 operands → two list elements.
const AT_PAIR: &str = "f>L n;xs=[10 20 30];[at xs 0 at xs 2]";
const MIN_MAX_PAIR: &str = "f>L n;a=2;b=5;[min a b max a b]";

fn check_binary_capped(engine: &str) {
    assert_eq!(
        run_ok(engine, AT_PAIR, "f", &[]),
        "[10, 30]",
        "two at calls (arity cap) {engine}",
    );
    assert_eq!(
        run_ok(engine, MIN_MAX_PAIR, "f", &[]),
        "[2, 5]",
        "min and max side-by-side {engine}",
    );
}

// --- Tier 4: multiple unary builtins side-by-side -----------------------

// The shape that bit the user: several `str`/`fmt`/etc. calls in a row.
// Each consumes exactly one operand.
const STR_TRIO: &str = "f>L t;a=1;b=2;c=3;[str a str b str c]";

fn check_multi(engine: &str) {
    assert_eq!(
        run_ok(engine, STR_TRIO, "f", &[]),
        "[1, 2, 3]",
        "three str calls as three list elements {engine}",
    );
}

// --- Tier 5: bare-ref behaviour preserved for locals --------------------

// Locals aren't in fn_arity, so `[a b c]` still yields 3 elements - the
// existing `regression_list_literal_refs.rs` invariant.
const BARE_LOCALS: &str = "f>L n;a=1;b=2;c=3;[a b c]";
const MIXED_LOCALS: &str = "f>L n;a=1;c=3;[a 2 c]";
const PURE_NUMBERS: &str = "f>L n;[1 2 3]";

fn check_locals_unchanged(engine: &str) {
    assert_eq!(
        run_ok(engine, BARE_LOCALS, "f", &[]),
        "[1, 2, 3]",
        "bare locals stay as 3 elements {engine}",
    );
    assert_eq!(
        run_ok(engine, MIXED_LOCALS, "f", &[]),
        "[1, 2, 3]",
        "mixed locals + literals stay as 3 elements {engine}",
    );
    assert_eq!(
        run_ok(engine, PURE_NUMBERS, "f", &[]),
        "[1, 2, 3]",
        "pure number list still 3 elements {engine}",
    );
}

// --- Tier 6: HOF fn-ref position preserved ------------------------------

// `map` has fn-ref slot 0, so `dbl` must stay a bare ref. The whole
// `[map dbl xs]` parses as one element: Call(map, [Ref(dbl), Ref(xs)]).
// `hd` of that one-element list gives the result of the map.
const MAP_HOF_IN_LIST: &str = "dbl x:n>n;*x 2
g>L n;xs=[1 2 3];hd [map dbl xs]";

fn check_hof_preserved(engine: &str) {
    // Tree only: VM/cranelift skip HOF dispatch (FnRef NaN-tagging TBD)
    if engine != "--run-tree" {
        return;
    }
    assert_eq!(
        run_ok(engine, MAP_HOF_IN_LIST, "g", &[]),
        "[2, 4, 6]",
        "map keeps fn-ref slot 0, no eager expansion {engine}",
    );
}

// --- Tier 7: nested-call within a single list element -------------------

// `[len hd xs]` parses as `[Call(len, [Call(hd, [xs])])]` because `hd` is
// also a known unary builtin and parse_call_arg's eager-expansion rule
// kicks in at the inner position. Result is a 1-element list of `len(hd(xs))`.
const NESTED_LEN_HD: &str = "f>L n;xs=[[1 2 3] [4 5]];[len hd xs]";

fn check_nested(engine: &str) {
    assert_eq!(
        run_ok(engine, NESTED_LEN_HD, "f", &[]),
        "[3]",
        "nested known-fn calls in single list element {engine}",
    );
}

// --- Cross-engine harness -----------------------------------------------

fn check_all(engine: &str) {
    check_repro(engine);
    check_unary(engine);
    check_binary_capped(engine);
    check_multi(engine);
    check_locals_unchanged(engine);
    check_hof_preserved(engine);
    check_nested(engine);
}

#[test]
fn listlit_fnref_greedy_tree() {
    check_all("--run-tree");
}

#[test]
fn listlit_fnref_greedy_vm() {
    check_all("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn listlit_fnref_greedy_cranelift() {
    check_all("--run-cranelift");
}
