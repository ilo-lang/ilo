//! Regression for ILO-473: braceless guards inside a lambda body are
//! rejected at parse time with ILO-P023.
//!
//! Pre-fix, writing `=cond v;rest` inside a lambda body silently
//! early-returned from the *enclosing* function — not the lambda. The
//! lambda body skipped past the guard and the outer caller returned
//! out from under the HOF. No diagnostic fired.
//!
//! This test pins:
//!   1. paren-lambda body with braceless guard  -> ILO-P023
//!   2. brace-lambda body with braceless guard  -> ILO-P023
//!   3. negated braceless guard inside lambda   -> ILO-P023
//!   4. prefix ternary inside lambda            -> parses fine
//!   5. braceless guard at top-level fn body    -> still works (no regression)
//!   6. braceless guard in nested lambda inside another lambda body -> ILO-P023

use ilo::ast::Span;
use ilo::lexer;
use ilo::parser::{self, ParseError};

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

fn try_parse(src: &str) -> Vec<ParseError> {
    let pairs = lex_to_pairs(src);
    let (_prog, errs) = parser::parse(pairs);
    errs
}

fn assert_code(src: &str, code: &str) {
    let errs = try_parse(src);
    assert!(
        errs.iter().any(|e| e.code == code),
        "expected {code} for {src:?}, got {:?}",
        errs.iter().map(|e| e.code).collect::<Vec<_>>()
    );
}

fn assert_ok(src: &str) {
    let errs = try_parse(src);
    assert!(errs.is_empty(), "expected ok for {src:?}, got {errs:?}");
}

#[test]
fn paren_lambda_with_braceless_guard_rejected() {
    // (x:n>n; >=x 0 0;x) — braceless guard would early-return from the
    // enclosing function. Pre-fix: silent miscompile. Post-fix: ILO-P023.
    assert_code("f xs:L n>L n;map (x:n>n;>=x 0 0;x) xs", "ILO-P023");
}

#[test]
fn brace_lambda_with_braceless_guard_rejected() {
    assert_code("f xs:L n>L n;map {x> >=x 0 0;x} xs", "ILO-P023");
}

#[test]
fn negated_braceless_guard_in_lambda_rejected() {
    assert_code("f xs:L n>L n;map (x:n>n;!>=x 0 0;x) xs", "ILO-P023");
}

#[test]
fn prefix_ternary_in_lambda_parses() {
    // Canonical replacement #1: prefix ternary `?cond then else`.
    assert_ok("f xs:L n>L n;map (x:n>n;?>=x 0 0 x) xs");
    assert_ok("f xs:L n>L n;map {x> ?>=x 0 0 x} xs");
}

#[test]
fn top_level_braceless_guard_still_works() {
    // Canonical use of a braceless guard: at fn body, NOT inside a lambda.
    assert_ok("classify sp:n>t;>=sp 1000 \"gold\";>=sp 500 \"silver\";\"bronze\"");
}

#[test]
fn nested_lambda_with_braceless_guard_rejected() {
    // Braceless guard nested two lambdas deep still errors.
    assert_code(
        "f xs:L L n>L L n;map (ys:L n>L n;map (x:n>n;>=x 0 0;x) ys) xs",
        "ILO-P023",
    );
}
