// ILO-456: HOF lambda surface inconsistency — targeted hints for the
// three rejected lambda shapes at HOF call-arg position. The two canonical
// forms (paren `(x:t>r;body)` and bare-param brace `{x> body}`) stay; the
// three rejected shapes (`{x:t> body}`, `\x:t>body`, `fn x:t>r;body`) each
// emit a diagnostic that names both canonical alternatives and a call-site
// rewrite, so the persona's first retry is the right one.
//
// Evidence (sse-chunk-parser, graphql-error-collector, api-key-rotation,
// cors-preflight-builder on 2026-05-23) showed agents trained on functional
// priors reaching for these shapes; before this change each surfaced with a
// generic ILO-P001/L001 that didn't name the fix.

use ilo::ast::Span;
use ilo::lexer;
use ilo::parser::{self, ParseError};

fn parse_first_err(src: &str) -> Option<ParseError> {
    let tokens = lexer::lex(src).ok()?;
    let pairs = tokens
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
        .collect::<Vec<_>>();
    parser::parse(pairs).1.into_iter().next()
}

fn lex_err_suggestion(src: &str) -> String {
    match lexer::lex(src) {
        Err(e) => e.suggestion,
        Ok(_) => String::new(),
    }
}

#[test]
fn typed_brace_lambda_emits_paren_form_hint() {
    // `flt {x:n> >x 0} xs` — typed-brace at HOF call site.
    let err = parse_first_err("f xs:Ln>Ln;flt {x:n> >x 0} xs").expect("expected an error");
    assert_eq!(err.code, "ILO-P001", "got {:?}", err);
    let hint = err.hint.unwrap_or_default();
    assert!(
        hint.contains("(x:t>r;body)") && hint.contains("{x> body}"),
        "hint must name both canonical forms; got: {hint}"
    );
    assert!(
        hint.contains("flt (x:"),
        "hint must include a call-site rewrite for the typed-paren form; got: {hint}"
    );
}

#[test]
fn backslash_typed_lambda_emits_canonical_form_hint() {
    // `flt \x:n>>x 0 xs` — backslash typed lambda. Hint comes from the
    // lexer because `\` is rejected by logos before the parser sees it.
    let s = lex_err_suggestion(r"f xs:Ln>Ln;flt \x:n>>x 0 xs");
    assert!(!s.is_empty(), "expected an ILO-L001 hint");
    assert!(
        s.contains("(x:t>r;body)") && s.contains("{x> body}"),
        "hint must name both canonical forms; got: {s}"
    );
    assert!(
        s.contains("map (x:") || s.contains("flt (x:") || s.contains("(x:n>n"),
        "hint must include a concrete call-site rewrite; got: {s}"
    );
}

#[test]
fn fn_keyword_inline_lambda_emits_canonical_form_hint() {
    // `flt fn x:n>n;>x 0 xs` — `fn`-keyword inline at call-arg position.
    let err = parse_first_err("f xs:Ln>Ln;flt fn x:n>n;>x 0 xs").expect("expected an error");
    assert_eq!(err.code, "ILO-P009", "got {:?}", err);
    let hint = err.hint.unwrap_or_default();
    assert!(
        hint.contains("(p:t>r;body)") && hint.contains("{p> body}"),
        "hint must name both canonical forms; got: {hint}"
    );
    assert!(
        hint.contains("flt (x:") || hint.contains("flt {x>"),
        "hint must include a call-site rewrite; got: {hint}"
    );
}

#[test]
fn canonical_paren_lambda_still_parses() {
    assert!(parse_first_err("f xs:Ln>Ln;flt (x:n>b;>x 0) xs").is_none());
}

#[test]
fn canonical_bare_brace_lambda_still_parses() {
    assert!(parse_first_err("f xs:Ln>Ln;flt {x> >x 0} xs").is_none());
}
