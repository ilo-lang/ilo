//! ILO-T043: recursive self-call at non-tail position discards its return.
//!
//! Surfaced 2026-05-21 by the interp1d persona (see #5ba / PR 571). Agent wrote
//! `find-idx xs t i:n>n; =t i i; find-idx xs t +i 1; -1` expecting the recursive
//! call to short-circuit. In fact the recursive call is non-tail (followed by
//! `-1`), so its return value is silently dropped and the fn always returns -1.
//! Verifier emitted no signal; the persona mis-diagnosed the bug as broken
//! braceless guards. T043 closes that signal gap.
//!
//! Scope: narrow — only fires for recursive self-calls (caller name == callee
//! name). Bare calls to *other* user fns at non-tail position are legitimate
//! when the callee is side-effecting (logging, file I/O) so we don't warn on
//! those. Broaden later if reruns surface other false-positive classes.

use ilo::ast::Span;
use ilo::{lexer, parser, verify};

fn warnings(code: &str) -> Vec<String> {
    let tokens = lexer::lex(code).expect("lex");
    let token_spans: Vec<(lexer::Token, Span)> = tokens
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
        .collect();
    let (program, parse_errors) = parser::parse(token_spans);
    assert!(
        parse_errors.is_empty(),
        "parse errors: {parse_errors:?} for source {code:?}"
    );
    let result = verify::verify(&program);
    result
        .warnings
        .into_iter()
        .filter(|w| w.code == "ILO-T043")
        .map(|w| w.message)
        .collect()
}

#[test]
fn non_tail_recursive_self_call_warns() {
    // The persona's exact shape. Recursive call is stmt 2 of 3, return value
    // discarded, fn falls through to `-1` literal on every iteration.
    let ws =
        warnings(r#"find-idx xs:L n target:n i:n>n; =target i i; find-idx xs target +i 1; -1"#);
    assert_eq!(ws.len(), 1, "expected one ILO-T043 warning, got {ws:?}");
    assert!(
        ws[0].contains("find-idx"),
        "message names callee: {}",
        ws[0]
    );
    assert!(
        ws[0].contains("non-tail"),
        "message names the cause: {}",
        ws[0]
    );
}

#[test]
fn tail_recursive_self_call_no_warn() {
    // Same fn but the recursive call IS the last statement — tail position.
    // No warning expected (this is the canonical accumulator shape).
    let ws = warnings(r#"find-idx xs:L n target:n i:n>n; =target i i; find-idx xs target +i 1"#);
    assert!(ws.is_empty(), "tail call must not warn, got {ws:?}");
}

#[test]
fn non_recursive_user_call_no_warn() {
    // A bare non-recursive user-fn call at non-tail position must NOT warn.
    // Start narrow: only self-calls are confidence-high. Other callers may be
    // legitimately side-effecting.
    let ws = warnings(
        r#"g x:n>n;+x 1
f x:n>n;g x; +x 1"#,
    );
    assert!(
        ws.is_empty(),
        "non-recursive call must not warn, got {ws:?}"
    );
}

#[test]
fn recursive_call_with_unwrap_ret_no_warn() {
    // `ret <call>` puts the call in tail position via explicit return — the
    // statement kind is Stmt::Return, not Stmt::Expr, so the body-level walker
    // never sees it as a discarded expression.
    let ws =
        warnings(r#"find-idx xs:L n target:n i:n>n; =target i i; ret find-idx xs target +i 1; -1"#);
    assert!(
        ws.is_empty(),
        "ret-wrapped recursive call must not warn, got {ws:?}"
    );
}

#[test]
fn multiple_non_tail_recursive_calls_each_warn() {
    // Two recursive calls in a row both at non-tail position. Both must warn
    // so the agent sees every footgun in one verify pass.
    let ws = warnings(r#"f xs:L n i:n>n; f xs +i 1; f xs +i 2; -1"#);
    assert_eq!(
        ws.len(),
        2,
        "expected two ILO-T043 warnings (one per non-tail recursive call), got {ws:?}"
    );
}
