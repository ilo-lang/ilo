//! Regression tests for VM-side tail-call elimination (OP_TAILCALL).
//!
//! PR1 of the TCO series added the tree-interpreter trampoline. PR2 adds
//! the bytecode VM's matching `OP_TAILCALL` opcode: when a static user-fn
//! call sits in tail position the compiler emits `OP_TAILCALL` instead of
//! `OP_CALL` + `OP_RET`, and the runtime reuses the current `CallFrame`
//! rather than pushing a new one.
//!
//! Without OP_TAILCALL the VM's `self.frames` Vec grows on every recursive
//! call. A 5M-deep count-down would allocate ~120MB of frame entries plus
//! the per-frame register window on `self.stack`. With OP_TAILCALL the
//! frame stack is O(1) — the same `CallFrame` slot is re-used in place
//! for every iteration.
//!
//! These tests drive the bytecode VM directly via the library API
//! (`ilo::vm::compile_and_run`) so the OP_TAILCALL path is exercised
//! deterministically — the CLI's default engine selection would otherwise
//! mask a regression that re-routes through a different backend.

use ilo::ast;
use ilo::lexer;
use ilo::parser;
use ilo::runtime::Value;
use ilo::vm;

fn run_vm(src: &str, func: &str, args: Vec<Value>) -> Value {
    let tokens = lexer::lex(src).expect("lex");
    let token_spans: Vec<(lexer::Token, ast::Span)> = tokens
        .into_iter()
        .map(|(t, r)| {
            (
                t,
                ast::Span {
                    start: r.start,
                    end: r.end,
                },
            )
        })
        .collect();
    let (mut program, parse_errors) = parser::parse(token_spans);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    ast::resolve_aliases(&mut program);
    ast::desugar_dot_var_index(&mut program);

    let compiled = vm::compile(&program).expect("vm::compile failed");
    vm::with_active_registry(&compiled, || {
        vm::run(&compiled, Some(func), args).expect("vm::run failed")
    })
}

/// Deep tail-recursive countdown on the VM. The recursive call is the
/// function's tail (the final statement is `count-down -n 1`), so the
/// compiler emits OP_TAILCALL and the runtime reuses the current frame.
/// A pre-OP_TAILCALL build allocates 1M `CallFrame` entries plus 1M
/// register windows on the stack; with OP_TAILCALL the frame stack stays
/// at 1.
#[test]
fn tco_vm_countdown_1m() {
    let src = "count-down n:n>n;=n 0 0;count-down -n 1\n";
    let result = run_vm(src, "count-down", vec![Value::Number(1_000_000.0)]);
    assert_eq!(result, Value::Number(0.0));
}

/// Match parity with the tree-walker's 5M depth. This is the headline
/// regression: without OP_TAILCALL the VM would allocate gigabytes of
/// frame state before OOM'ing or paging itself to death; with
/// OP_TAILCALL it runs in tens of milliseconds on constant memory.
#[test]
fn tco_vm_countdown_5m() {
    let src = "count-down n:n>n;=n 0 0;count-down -n 1\n";
    let result = run_vm(src, "count-down", vec![Value::Number(5_000_000.0)]);
    assert_eq!(result, Value::Number(0.0));
}

/// Tail-recursive accumulator over a 100k-element list. Exercises
/// OP_TAILCALL on a body whose tail call carries multiple args (`xs`
/// and the running `acc`). RC discipline matters here: `tl xs` produces
/// a fresh list NanVal whose RC is owned by the local that the new
/// frame inherits, and the outgoing accumulator slot has to be released
/// without double-dropping the arg the next frame takes over.
///
/// Sum of 0..100000 = 100000 * 99999 / 2 = 4_999_950_000.
#[test]
fn tco_vm_sum_list_accumulator_100k() {
    let src = "\
sum-acc xs:L n acc:n>n;empty=len xs;=empty 0 acc;sum-acc tl xs +acc hd xs

main >n;xs=rng 0 100000;sum-acc xs 0
";
    let result = run_vm(src, "main", vec![]);
    assert_eq!(result, Value::Number(4_999_950_000.0));
}

/// Cross-function tail call: `ev` tail-calls `od` tail-calls `ev` …
/// 1M deep. Exercises OP_TAILCALL's `chunk_idx` switch path — the
/// reused frame's chunk pointer flips between two functions every
/// iteration. A bug in the chunk swap (e.g. forgetting to update
/// `frame.chunk_idx` or `ci`) would either crash or loop forever.
#[test]
fn tco_vm_mutual_recursion_1m() {
    // ev(n) returns 1 when n hits zero, 0 when od(n) hits zero. The
    // bodies use plain number literals as the bool surrogates (ilo's
    // narrow bool channel is not the point of the test). `ev 1_000_000`
    // bounces 1M times and lands in `ev` with n=0 (1_000_000 is even),
    // so the chain returns Number(1.0).
    let src = "\
ev n:n>n;=n 0 1;od -n 1
od n:n>n;=n 0 0;ev -n 1
main >n;ev 1000000
";
    let result = run_vm(src, "main", vec![]);
    assert_eq!(result, Value::Number(1.0));
}

/// Three-function cycle: `f` tail-calls `g`, `g` tail-calls `h`, `h`
/// tail-calls `f`. Confirms `OP_TAILCALL` handles longer chunk-swap
/// chains, not just self-recursion or two-way mutual recursion.
#[test]
fn tco_vm_three_way_tail_chain_1m() {
    // f, g, h each carry their own base case so the chain terminates
    // regardless of where n hits zero. The recursion order is
    // f -> g -> h -> f -> g -> h ... ; main starts at f(1_000_000), so
    // step k lands in f when k % 3 == 0, g when k % 3 == 1, h when
    // k % 3 == 2. n decreases by 1 each step, so n = 0 happens at step
    // k = 1_000_000. 1_000_000 % 3 == 1, so the terminator fires inside
    // g, returning 13. The whole 999_999-step descent runs on a single
    // reused frame thanks to OP_TAILCALL.
    let src = "\
f n:n>n;=n 0 7;g -n 1
g n:n>n;=n 0 13;h -n 1
h n:n>n;=n 0 19;f -n 1
main >n;f 1000000
";
    let result = run_vm(src, "main", vec![]);
    assert_eq!(result, Value::Number(13.0));
}

/// Tail-call through a braceless guard. The base case is the
/// early-return branch; the recursive case is the function's tail.
/// Confirms `in_tail_position` propagates through the guard the same
/// way the tree-walker's trampoline does.
#[test]
fn tco_vm_through_braceless_guard() {
    let src = "go n:n>n;=n 0 42;go -n 1\n";
    let result = run_vm(src, "go", vec![Value::Number(500_000.0)]);
    assert_eq!(result, Value::Number(42.0));
}
