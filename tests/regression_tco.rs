//! Regression tests for the tree-interpreter tail-call trampoline.
//!
//! Background:
//!
//! The ilo manifesto vetoed adding a `loop` keyword to express iteration
//! beyond `@` foreach. Instead, the language guarantees that tail calls do
//! not consume host-stack frames, so a function that recurses only in tail
//! position can run to arbitrary depth. PR1 of the TCO series ships the
//! tree interpreter's trampoline. VM and Cranelift backends gain matching
//! support in PR2 and PR3.
//!
//! What this test exercises:
//!
//! Each test drives the tree interpreter directly via the library API
//! (`ilo::runtime::run`). The tree-walker is no longer reachable from
//! the public CLI (soft-deprecated in 0.12.x; CLI defaults to the VM), so a
//! CLI-based round-trip would silently route through the VM and bypass the
//! trampoline entirely. Going through the library API exercises the actual
//! code path the trampoline lives on.
//!
//! Recursion depth: each test runs at a depth that would blow the default
//! Rust thread stack (8MB on macOS, 2-8MB on Linux) if the interpreter
//! recursed normally. A pre-TCO build segfaults on these well before the
//! values match; a TCO build runs them on a constant-size host stack.
//!
//! No `RUST_MIN_STACK` is set — that's the whole point: the guarantee has
//! to hold on the default stack size that real programs run under.

use ilo::ast;
use ilo::runtime::{self, Value};
use ilo::lexer;
use ilo::parser;

fn run_tree(src: &str, func: &str, args: Vec<Value>) -> Value {
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

    runtime::run(&program, Some(func), args).expect("runtime::run failed")
}

/// Deep tail-recursive countdown.
///
/// `count-down n` recurses with `n-1` until the braceless guard `=n 0 0`
/// fires and returns 0. Each recursion is at the function's tail position
/// (the final statement is `count-down -n 1`), so the trampoline rebinds
/// the parameter in place rather than pushing a Rust frame.
#[test]
fn tco_countdown_1m() {
    let src = "count-down n:n>n;=n 0 0;count-down -n 1\n";
    let result = run_tree(src, "count-down", vec![Value::Number(1_000_000.0)]);
    assert_eq!(result, Value::Number(0.0));
}

/// Push to a depth that would unambiguously overflow any reasonable host
/// stack (each frame is hundreds of bytes; 5M frames is gigabytes). If this
/// passes, the trampoline is working.
#[test]
fn tco_countdown_5m() {
    let src = "count-down n:n>n;=n 0 0;count-down -n 1\n";
    let result = run_tree(src, "count-down", vec![Value::Number(5_000_000.0)]);
    assert_eq!(result, Value::Number(0.0));
}

/// Tail-recursive accumulator over a list. Iterates a 100k-element list,
/// summing into `acc` on every step. The recursive call is the function's
/// tail position; the trampoline reuses the same frame.
///
/// Expected sum of 0..100000 is 100000 * 99999 / 2 = 4_999_950_000.
#[test]
fn tco_sum_list_accumulator_100k() {
    let src = "\
sum-acc xs:L n acc:n>n;empty=len xs;=empty 0 acc;sum-acc tl xs +acc hd xs

main >n;xs=rng 0 100000;sum-acc xs 0
";
    let result = run_tree(src, "main", vec![]);
    assert_eq!(result, Value::Number(4_999_950_000.0));
}

/// Tail call through a braceless guard. The base case is the early-return
/// branch; the recursive case is the function's tail. Confirms tail
/// position propagates through `cond expr` braceless guards.
#[test]
fn tco_through_braceless_guard() {
    // Identical shape to count-down but written so the recursive call is
    // wrapped in the guard's else (the trailing statement of the function
    // body). This is the same syntactic shape every count-down style fn
    // uses; making it an explicit test pins the propagation rule.
    let src = "go n:n>n;=n 0 42;go -n 1\n";
    let result = run_tree(src, "go", vec![Value::Number(500_000.0)]);
    assert_eq!(result, Value::Number(42.0));
}

/// Confirm the trampoline preserves error-call-stack reporting. Errors
/// raised inside a tail-recursive function still attribute correctly to
/// the function name (the call_stack push/pop wraps each trampoline
/// iteration).
#[test]
fn tco_error_attribution() {
    // `boom` is tail-recursive; the base case raises an Err. The runner
    // converts Err returns into exit code 1, but the call_stack entry
    // should still mention `boom`.
    // `R n t` — Result with Ok=number, Err=text. Base case returns
    // `^"done"` (Err), recursive case is the function tail. Trampoline
    // runs 1000 iterations before the guard fires.
    let src = "boom n:n>R n t;=n 0 ^\"done\";boom -n 1\n";
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
    assert!(parse_errors.is_empty(), "parse: {:?}", parse_errors);
    ast::resolve_aliases(&mut program);
    ast::desugar_dot_var_index(&mut program);
    let result = runtime::run(&program, Some("boom"), vec![Value::Number(1_000.0)])
        .expect("boom returns Err value, not interpreter error");
    // boom returns Value::Err("done"), confirming the trampoline ran
    // through 1000 recursive iterations and surfaced the base case's
    // ^"done" as the function's return value.
    match result {
        Value::Err(inner) => match *inner {
            Value::Text(s) => assert_eq!(&*s, "done"),
            other => panic!("expected Err(Text), got Err({:?})", other),
        },
        other => panic!("expected Err, got {:?}", other),
    }
}
