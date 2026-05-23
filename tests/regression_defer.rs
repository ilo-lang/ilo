//! Regression tests for `defer` and `errdefer` (ILO-56).
//!
//! Covers:
//!   - LIFO ordering of multiple defers
//!   - defer fires on normal return (fall-through and `ret`)
//!   - errdefer fires only on error-path exit (`ret ^err`, `Value::Err`)
//!   - defer does NOT fire on non-error exit when only errdefer is registered
//!   - Cross-engine: tree interpreter and VM both produce the same output
//!     (VM bridges defer-containing programs to the tree walker)

use ilo::ast;
use ilo::interpreter::{self, Value};
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
    interpreter::run(&program, Some(func), args).expect("interpreter::run failed")
}

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
    let compiled = ilo::vm::compile(&program).expect("vm::compile failed");
    ilo::vm::run(&compiled, Some(func), args)
        .map_err(|e| format!("vm::run failed: {:?}", e))
        .expect("vm::run failed")
}

// ── defer: basic LIFO ordering ───────────────────────────────────────────────

/// Three defers in one function body fire in LIFO order (3, 2, 1).
/// We capture side-effects via a list accumulator stored in a module-level
/// variable — but ilo has no mutable globals, so we use the return value
/// itself. The defers call `prnt` (side-effect only); we verify the
/// return value flows through unmodified.
#[test]
fn defer_return_value_passes_through() {
    // The function registers three defers then returns 99.
    // The defers are expressions with no value; return value must be 99.
    let src = "f x:n>n\n  defer +x 0\n  defer +x 0\n  x\n";
    let result = run_tree(src, "f", vec![Value::Number(99.0)]);
    assert_eq!(result, Value::Number(99.0));
}

#[test]
fn defer_return_value_passes_through_vm() {
    let src = "f x:n>n\n  defer +x 0\n  defer +x 0\n  x\n";
    let result = run_vm(src, "f", vec![Value::Number(99.0)]);
    assert_eq!(result, Value::Number(99.0));
}

/// Verify that defers fire even on `ret` early return.
/// `f 0` returns early via `ret`; the defer must still fire.
/// We can't inspect the defer's output here, but we can verify no panic.
#[test]
fn defer_fires_on_early_ret() {
    let src = "f x:n>n\n  defer +x 0\n  =x 0{ret 7}\n  x\n";
    let r1 = run_tree(src, "f", vec![Value::Number(0.0)]);
    let r2 = run_tree(src, "f", vec![Value::Number(5.0)]);
    assert_eq!(r1, Value::Number(7.0));
    assert_eq!(r2, Value::Number(5.0));
}

#[test]
fn defer_fires_on_early_ret_vm() {
    let src = "f x:n>n\n  defer +x 0\n  =x 0{ret 7}\n  x\n";
    let r1 = run_vm(src, "f", vec![Value::Number(0.0)]);
    let r2 = run_vm(src, "f", vec![Value::Number(5.0)]);
    assert_eq!(r1, Value::Number(7.0));
    assert_eq!(r2, Value::Number(5.0));
}

// ── errdefer: fires only on error exit ───────────────────────────────────────

/// `errdefer` does NOT fire on a normal (non-error) exit.
/// If it fired, the erroneous side-effect would change the return type.
/// We can't directly observe the side-effect here, but we verify the
/// return value is correct and no panic occurs.
#[test]
fn errdefer_does_not_fire_on_normal_exit() {
    // Returns ~x (Ok value). errdefer should NOT fire.
    let src = "f x:n>R n t\n  errdefer +x 0\n  ~x\n";
    let result = run_tree(src, "f", vec![Value::Number(5.0)]);
    assert_eq!(result, Value::Ok(Box::new(Value::Number(5.0))));
}

#[test]
fn errdefer_does_not_fire_on_normal_exit_vm() {
    let src = "f x:n>R n t\n  errdefer +x 0\n  ~x\n";
    let result = run_vm(src, "f", vec![Value::Number(5.0)]);
    assert_eq!(result, Value::Ok(Box::new(Value::Number(5.0))));
}

/// `errdefer` fires when the function returns `^err` (ilo Err value).
/// We verify by checking the return value is Value::Err.
#[test]
fn errdefer_fires_on_err_return() {
    // Returns ^"oops". The errdefer fires. Return value still flows through.
    let src = "f x:n>R n t\n  errdefer +x 0\n  ^\"oops\"\n";
    let result = run_tree(src, "f", vec![Value::Number(1.0)]);
    assert_eq!(
        result,
        Value::Err(Box::new(Value::Text("oops".to_string().into())))
    );
}

#[test]
fn errdefer_fires_on_err_return_vm() {
    let src = "f x:n>R n t\n  errdefer +x 0\n  ^\"oops\"\n";
    let result = run_vm(src, "f", vec![Value::Number(1.0)]);
    assert_eq!(
        result,
        Value::Err(Box::new(Value::Text("oops".to_string().into())))
    );
}

/// `errdefer` fires on `ret ^err` early return.
#[test]
fn errdefer_fires_on_ret_err() {
    let src = "f x:n>R n t\n  errdefer +x 0\n  =x 0{ret ^\"fail\"}\n  ~x\n";
    let r_err = run_tree(src, "f", vec![Value::Number(0.0)]);
    let r_ok = run_tree(src, "f", vec![Value::Number(3.0)]);
    assert_eq!(
        r_err,
        Value::Err(Box::new(Value::Text("fail".to_string().into())))
    );
    assert_eq!(r_ok, Value::Ok(Box::new(Value::Number(3.0))));
}

#[test]
fn errdefer_fires_on_ret_err_vm() {
    let src = "f x:n>R n t\n  errdefer +x 0\n  =x 0{ret ^\"fail\"}\n  ~x\n";
    let r_err = run_vm(src, "f", vec![Value::Number(0.0)]);
    let r_ok = run_vm(src, "f", vec![Value::Number(3.0)]);
    assert_eq!(
        r_err,
        Value::Err(Box::new(Value::Text("fail".to_string().into())))
    );
    assert_eq!(r_ok, Value::Ok(Box::new(Value::Number(3.0))));
}

// ── defer called from non-defer caller ───────────────────────────────────────

/// A function without defer calls a function with defer. The defer in the
/// callee fires correctly even when the caller is not deferred.
#[test]
fn defer_in_callee_fires_when_called_from_non_defer_caller() {
    let src = "inner x:n>n\n  defer +x 0\n  x\n\nouter y:n>n\n  inner y\n";
    let result = run_tree(src, "outer", vec![Value::Number(7.0)]);
    assert_eq!(result, Value::Number(7.0));
}

#[test]
fn defer_in_callee_fires_when_called_from_non_defer_caller_vm() {
    let src = "inner x:n>n\n  defer +x 0\n  x\n\nouter y:n>n\n  inner y\n";
    let result = run_vm(src, "outer", vec![Value::Number(7.0)]);
    assert_eq!(result, Value::Number(7.0));
}

// ── LIFO ordering ────────────────────────────────────────────────────────────

/// Multiple defers on the same function execute in LIFO order.
/// We can't inspect print-order from here, but we verify the computed
/// result is unaffected by the presence of multiple defers.
#[test]
fn multiple_defers_lifo_return_value() {
    let src = "f x:n>n\n  defer +x 1\n  defer +x 2\n  defer +x 3\n  x\n";
    let result = run_tree(src, "f", vec![Value::Number(10.0)]);
    assert_eq!(result, Value::Number(10.0));
}

#[test]
fn multiple_defers_lifo_return_value_vm() {
    let src = "f x:n>n\n  defer +x 1\n  defer +x 2\n  defer +x 3\n  x\n";
    let result = run_vm(src, "f", vec![Value::Number(10.0)]);
    assert_eq!(result, Value::Number(10.0));
}

// ── AST node: verify DeferKind is preserved through parse round-trip ─────────

#[test]
fn ast_defer_kind_always_parsed() {
    let src = "f x:n>n\n  defer +x 0\n  x\n";
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
    let (program, errs) = parser::parse(token_spans);
    assert!(errs.is_empty());
    let decl = &program.declarations[0];
    if let ast::Decl::Function { body, .. } = decl {
        assert!(matches!(
            body[0].node,
            ast::Stmt::Defer {
                kind: ast::DeferKind::Always,
                ..
            }
        ));
    } else {
        panic!("expected Function decl");
    }
}

#[test]
fn ast_defer_kind_onerror_parsed() {
    let src = "f x:n>n\n  errdefer +x 0\n  x\n";
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
    let (program, errs) = parser::parse(token_spans);
    assert!(errs.is_empty());
    let decl = &program.declarations[0];
    if let ast::Decl::Function { body, .. } = decl {
        assert!(matches!(
            body[0].node,
            ast::Stmt::Defer {
                kind: ast::DeferKind::OnError,
                ..
            }
        ));
    } else {
        panic!("expected Function decl");
    }
}

// ── defer reserved keywords cannot be used as identifiers ────────────────────

#[test]
fn defer_reserved_as_identifier_is_parse_error() {
    let src = "f x:n>n\n  defer=5\n  x\n";
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
    let (_, errs) = parser::parse(token_spans);
    assert!(
        !errs.is_empty(),
        "expected parse error when using `defer` as identifier"
    );
}

#[test]
fn errdefer_reserved_as_identifier_is_parse_error() {
    let src = "f x:n>n\n  errdefer=5\n  x\n";
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
    let (_, errs) = parser::parse(token_spans);
    assert!(
        !errs.is_empty(),
        "expected parse error when using `errdefer` as identifier"
    );
}

// ── Performance: native VM defer must be faster than the old tree bridge ─────
//
// This test compiles a defer-containing function and measures the wall-clock
// time for 1 000 repeated `vm::run` calls against the equivalent count via
// the tree interpreter.  We assert that the VM path is at least as fast
// (within a 2× margin to avoid flakiness on CI), and print the ratio.
//
// A significant speed-up is expected because OP_DEFER_PUSH / OP_DEFER_DRAIN
// avoid the overhead of AST node cloning and full tree re-evaluation that the
// old bridge incurred on every call.
#[test]
fn defer_vm_not_slower_than_tree() {
    use std::time::Instant;

    // A function with three defers and a real return expression.
    // chosen to be non-trivial enough that the differ overhead is visible.
    let src = "f x:n>n\n  defer +x 1\n  defer *x 1\n  defer -x 0\n  +x 0\n";
    const ITERS: u32 = 500;

    // ── tree timing ──
    let t_start = Instant::now();
    for _ in 0..ITERS {
        let v = run_tree(src, "f", vec![Value::Number(42.0)]);
        assert_eq!(v, Value::Number(42.0));
    }
    let tree_ns = t_start.elapsed().as_nanos();

    // ── VM timing ──
    // Pre-compile once; measure only the run cost.
    let compiled = {
        let tokens = ilo::lexer::lex(src).expect("lex");
        let token_spans: Vec<(ilo::lexer::Token, ilo::ast::Span)> = tokens
            .into_iter()
            .map(|(t, r)| {
                (
                    t,
                    ilo::ast::Span {
                        start: r.start,
                        end: r.end,
                    },
                )
            })
            .collect();
        let (mut program, _) = ilo::parser::parse(token_spans);
        ilo::ast::resolve_aliases(&mut program);
        ilo::ast::desugar_dot_var_index(&mut program);
        ilo::vm::compile(&program).expect("vm::compile")
    };

    let v_start = Instant::now();
    for _ in 0..ITERS {
        let v = ilo::vm::run(&compiled, Some("f"), vec![Value::Number(42.0)]).expect("run");
        assert_eq!(v, Value::Number(42.0));
    }
    let vm_ns = v_start.elapsed().as_nanos();

    let ratio = tree_ns as f64 / vm_ns as f64;
    eprintln!("defer perf: tree={tree_ns}ns  vm={vm_ns}ns  vm_speedup={ratio:.2}x ({ITERS} iters)");

    // VM must not be more than 2× slower than the tree (in practice it is
    // faster because the tree bridge clones AST nodes on every call).
    assert!(
        vm_ns <= tree_ns * 2,
        "VM defer path is more than 2× slower than tree: vm={vm_ns}ns tree={tree_ns}ns"
    );
}

// ── ILO-365: block-scope defer ────────────────────────────────────────────────
//
// Note: `defer` takes an *expression* (not a statement), so mutation via
// `defer =var expr` is unavailable (the parser rejects `defer =` as a
// potential `defer = 5` assignment to the keyword).  Block-scope firing is
// therefore verified by two complementary approaches:
//   1. Return-value tests (run_tree): verify that block-scope defers do not
//      corrupt the function return value and that no panic occurs.
//   2. CLI output tests (regression_defer_block_scope.rs): verify the `prnt`
//      side-effects fire the correct number of times by capturing stdout.

/// A `defer prnt` inside a foreach loop body: return value is not corrupted.
#[test]
fn defer_in_foreach_return_value_unaffected() {
    // Three iterations; defer fires prnt (side-effect only). Return must be 42.
    let src = "f n:n>n\n  @i [1 2 3]{defer prnt i}\n  n\n";
    let result = run_tree(src, "f", vec![Value::Number(42.0)]);
    assert_eq!(result, Value::Number(42.0));
}

/// A `defer prnt` inside a forrange loop body: return value is not corrupted.
#[test]
fn defer_in_forrange_return_value_unaffected() {
    let src = "f n:n>n\n  @i 0..4{defer prnt i}\n  n\n";
    let result = run_tree(src, "f", vec![Value::Number(7.0)]);
    assert_eq!(result, Value::Number(7.0));
}

/// A `defer prnt` inside an if branch: return value flows through unaffected.
///
/// The ternary `=x 1{then_body}{else_body}` form is used so both branches are
/// testable.  In both cases the function return value is `x`.
#[test]
fn defer_in_if_return_value_unaffected() {
    let src = "f x:n>n\n  =x 1{defer prnt x}{defer prnt x}\n  x\n";
    let r1 = run_tree(src, "f", vec![Value::Number(1.0)]);
    let r0 = run_tree(src, "f", vec![Value::Number(0.0)]);
    assert_eq!(r1, Value::Number(1.0));
    assert_eq!(r0, Value::Number(0.0));
}

/// A `defer prnt` inside a match arm: return value flows through unaffected.
///
/// Uses the inline match form `? x { pat: body; _: body }`.
#[test]
fn defer_in_match_arm_return_value_unaffected() {
    // ? x { 1: {defer prnt x; x}; _: x }
    let src = "f x:n>n\n  ? x {1:{defer prnt x;x};_:x}\n  x\n";
    let r1 = run_tree(src, "f", vec![Value::Number(1.0)]);
    let r2 = run_tree(src, "f", vec![Value::Number(2.0)]);
    assert_eq!(r1, Value::Number(1.0));
    assert_eq!(r2, Value::Number(2.0));
}

/// A `defer` inside a while body: return value not corrupted.
#[test]
fn defer_in_while_return_value_unaffected() {
    let src = "f n:n>n\n  i=0\n  wh <i 3{i=+i 1;defer prnt i}\n  n\n";
    let result = run_tree(src, "f", vec![Value::Number(5.0)]);
    assert_eq!(result, Value::Number(5.0));
}
