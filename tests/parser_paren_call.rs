//! Regression tests for paren-form call syntax (ILO-51).
//!
//! `f(a, b)` parses identically to `f a b` — same `Expr::Call` AST node.
//! Disambiguation: adjacency (`f(` no space) triggers paren-call;
//! a space (`f (`) keeps `(...)` as a grouped-expression argument.

use ilo::ast::{Decl, Expr, Span, Stmt};
use ilo::lexer;
use ilo::parser;

fn lex_to_pairs(src: &str) -> Vec<(lexer::Token, Span)> {
    let tokens = lexer::lex(src).expect("lex failed");
    tokens
        .into_iter()
        .map(|(t, r)| {
            (
                t,
                Span {
                    start: r.start,
                    end: r.end,
                },
            )
        })
        .collect()
}

/// Parse `src` and assert no parse errors. Returns the program.
fn parse_ok(src: &str) -> ilo::ast::Program {
    let pairs = lex_to_pairs(src);
    let (prog, errs) = parser::parse(pairs);
    assert!(
        errs.is_empty(),
        "unexpected parse errors for {src:?}: {errs:?}"
    );
    prog
}

/// Extract the first body statement's expression from the first `Decl::Function`.
fn first_body_expr(prog: &ilo::ast::Program) -> &Expr {
    let Some(Decl::Function { body, .. }) = prog.declarations.first() else {
        panic!("first decl is not a Function")
    };
    let Some(stmt) = body.first() else {
        panic!("function body is empty")
    };
    let Stmt::Expr(expr) = &stmt.node else {
        panic!("first body statement is not an Expr")
    };
    expr
}

// ---------------------------------------------------------------------------
// Core equivalence: paren-form produces identical Call AST to postfix form
// ---------------------------------------------------------------------------

#[test]
fn paren_2arg_same_ast_as_postfix() {
    let paren = parse_ok(r#"f s:t > L t; spl(s, ",")"#);
    let postfix = parse_ok(r#"f s:t > L t; spl s ",""#);
    assert_eq!(
        first_body_expr(&paren),
        first_body_expr(&postfix),
        "paren-form and postfix-form must produce identical AST"
    );
}

#[test]
fn paren_1arg_same_ast_as_postfix() {
    let paren = parse_ok("f x:n > n; abs(x)");
    let postfix = parse_ok("f x:n > n; abs x");
    assert_eq!(
        first_body_expr(&paren),
        first_body_expr(&postfix),
        "single-arg paren-call must equal postfix call"
    );
}

#[test]
fn paren_3arg_same_ast_as_postfix() {
    let paren = parse_ok("f a:n b:n c:n > n; spl(a, b, c)");
    let postfix = parse_ok("f a:n b:n c:n > n; spl a b c");
    assert_eq!(
        first_body_expr(&paren),
        first_body_expr(&postfix),
        "3-arg paren-call must equal postfix call"
    );
}

// ---------------------------------------------------------------------------
// Single-arg cases
// ---------------------------------------------------------------------------

#[test]
fn paren_single_arg_is_call_not_grouped() {
    // `abs(x)` — adjacent — must be a Call, not `abs` followed by grouped `(x)`
    let prog = parse_ok("f x:n > n; abs(x)");
    let expr = first_body_expr(&prog);
    let Expr::Call { function, args, .. } = expr else {
        panic!("expected Call, got {expr:?}");
    };
    assert_eq!(function, "abs");
    assert_eq!(args.len(), 1);
    assert!(matches!(&args[0], Expr::Ref(n) if n == "x"));
}

#[test]
fn paren_space_before_paren_is_grouped_arg() {
    // `abs (x)` — space before `(` — should still parse as Call abs with arg x,
    // because `(x)` is a grouped expr that evaluates to `x`. The AST is the same.
    let prog = parse_ok("f x:n > n; abs (x)");
    let expr = first_body_expr(&prog);
    let Expr::Call { function, args, .. } = expr else {
        panic!("expected Call, got {expr:?}");
    };
    assert_eq!(function, "abs");
    assert_eq!(args.len(), 1);
}

// ---------------------------------------------------------------------------
// Nested paren-calls
// ---------------------------------------------------------------------------

#[test]
fn nested_paren_calls() {
    // `f(g(x))` — nested paren-calls
    let prog = parse_ok("f x:n > n; abs(sqrt(x))");
    let expr = first_body_expr(&prog);
    let Expr::Call { function, args, .. } = expr else {
        panic!("expected outer Call, got {expr:?}");
    };
    assert_eq!(function, "abs");
    assert_eq!(args.len(), 1);
    let Expr::Call {
        function: inner_fn, ..
    } = &args[0]
    else {
        panic!("expected inner Call, got {:?}", args[0]);
    };
    assert_eq!(inner_fn, "sqrt");
}

// ---------------------------------------------------------------------------
// Trailing comma
// ---------------------------------------------------------------------------

#[test]
fn trailing_comma_accepted() {
    // `spl(s, ",",)` — trailing comma is accepted
    let prog = parse_ok(r#"f s:t > L t; spl(s, ",")"#);
    let expr = first_body_expr(&prog);
    assert!(matches!(expr, Expr::Call { .. }));
}

// ---------------------------------------------------------------------------
// Grouped expression as argument inside paren-call
// ---------------------------------------------------------------------------

#[test]
fn grouped_expr_as_arg_in_paren_call() {
    // `spl(s, (","))` — second arg is a grouped expression that evaluates to ","
    let prog = parse_ok(r#"f s:t > L t; spl(s, (","))"#);
    let expr = first_body_expr(&prog);
    let Expr::Call { function, args, .. } = expr else {
        panic!("expected Call, got {expr:?}");
    };
    assert_eq!(function, "spl");
    assert_eq!(args.len(), 2);
}

// ---------------------------------------------------------------------------
// Paren-call in operand position (inside expressions)
// ---------------------------------------------------------------------------

#[test]
fn paren_call_in_prefix_binary_expr() {
    // `+ 1 abs(x)` — paren-call as right operand of prefix binary op
    let prog = parse_ok("f x:n > n; + 1 abs(x)");
    let expr = first_body_expr(&prog);
    let Expr::BinOp { right, .. } = expr else {
        panic!("expected BinOp, got {expr:?}");
    };
    assert!(matches!(right.as_ref(), Expr::Call { function, .. } if function == "abs"));
}

// ---------------------------------------------------------------------------
// Zero-arg paren call (existing behaviour must be unaffected)
// ---------------------------------------------------------------------------

#[test]
fn zero_arg_paren_call_unaffected() {
    let prog = parse_ok("f > n; rnd()");
    let expr = first_body_expr(&prog);
    let Expr::Call { function, args, .. } = expr else {
        panic!("expected Call, got {expr:?}");
    };
    assert_eq!(function, "rnd");
    assert_eq!(args.len(), 0);
}

// ---------------------------------------------------------------------------
// fld (lambda) form: space before paren is NOT a paren-call
// ---------------------------------------------------------------------------

#[test]
fn fld_space_lambda_not_paren_call() {
    // `fld (x:n acc:n > n; + acc x) xs init`
    // `fld` has a SPACE before `(` — must be postfix call with lambda as first arg
    let prog = parse_ok("f xs:L n init:n > n; fld (x:n acc:n > n; + acc x) xs init");
    let expr = first_body_expr(&prog);
    let Expr::Call { function, args, .. } = expr else {
        panic!("expected Call, got {expr:?}");
    };
    assert_eq!(function, "fld");
    // fld should have 3 args: (lambda), xs, init
    assert_eq!(
        args.len(),
        3,
        "fld must have 3 args: lambda, collection, init"
    );
}
