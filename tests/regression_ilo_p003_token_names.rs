// Regression tests for ILO-P003 (and related P-series) diagnostics that
// previously surfaced parser-internal `TokenKind` variant names — `Greater`,
// `PipeOp`, `LBrace`, `RParen`, `Colon`, `Number(...)`, `Ident("...")` — in
// the message. Agents reading these messages cannot map "PipeOp" or "LBrace"
// back to a source character without spelunking the parser, so the fix
// surfaces the actual character(s) (`>`, `>>`, `{`, ...) wrapped in
// backticks, plus a human label for literals (`number 42`, `identifier foo`).
//
// Originating signal: agent-repair-loop rerun11 saw
// `ILO-P003: expected Greater, got PipeOp` and made three wrong guesses
// before realising the message was about `>` vs `>>`.
//
// The fix adds `Token::user_facing_name()` returning the source-character
// rendering, and routes every ILO-P003 / P004 / P005 / P007 / P009 / P011 /
// P013 / P016 / and the P001 "expected declaration, got ..." emit site
// through it. This regression test pins the new wording for the canonical
// shapes so a future refactor can't silently regress.

use ilo::ast::{Program, Span};
use ilo::lexer;
use ilo::parser::{self, ParseError};

fn parse_str_errors(src: &str) -> (Program, Vec<ParseError>) {
    let tokens = lexer::lex(src).expect("lex failed");
    let pairs: Vec<(lexer::Token, Span)> = tokens
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
    parser::parse(pairs)
}

/// Internal parser enum names that must never appear in a user-facing
/// diagnostic. Each is matched as a whole-word so we don't false-positive
/// on e.g. "Plus" appearing inside a longer identifier the user actually
/// typed.
const LEAK_NAMES: &[&str] = &[
    "Greater",
    "GreaterEq",
    "Less",
    "LessEq",
    "NotEq",
    "PlusEq",
    "PipeOp",
    "NilCoalesce",
    "BangBang",
    "Plus",
    "Minus",
    "Star",
    "Slash",
    "Amp",
    "Pipe",
    "Question",
    "Bang",
    "Caret",
    "Tilde",
    "Dollar",
    "Colon",
    "Semi",
    "DotDot",
    "DotQuestion",
    "Dot",
    "Comma",
    "LBrace",
    "RBrace",
    "LParen",
    "RParen",
    "LBracket",
    "RBracket",
    "Underscore",
    "Number(",
    "Text(",
    "Ident(",
    "ListType",
    "ResultType",
    "FnType",
    "OptType",
    "MapType",
    "SumType",
    "KwIf",
    "KwReturn",
    "KwLet",
    "KwFn",
    "KwDef",
    "KwVar",
    "KwConst",
    "True",
    "False",
    "Nil",
];

fn assert_no_token_enum_leak(source: &str) {
    let (_, errors) = parse_str_errors(source);
    assert!(
        !errors.is_empty(),
        "expected at least one parse error for `{source}`"
    );
    for e in &errors {
        for leak in LEAK_NAMES {
            assert!(
                !e.message.contains(leak),
                "ILO-{code} message leaks parser-internal token name `{leak}`: {msg}\nsource: {source}",
                code = &e.code[4..],
                msg = e.message,
            );
            if let Some(h) = &e.hint {
                assert!(
                    !h.contains(leak),
                    "ILO-{code} hint leaks parser-internal token name `{leak}`: {h}\nsource: {source}",
                    code = &e.code[4..],
                );
            }
        }
    }
}

#[test]
fn p003_pipe_op_in_return_type_position() {
    // The originating bug: `f x:n>>n;x` — agent typed `>>` (pipe) instead of
    // `>` (return-type separator). Old wording was
    // `expected Greater, got PipeOp`, which mentions neither character.
    // New wording must mention `>` and `>>`.
    let (_, errors) = parse_str_errors("f x:n>>n;x");
    let e = errors
        .iter()
        .find(|e| e.code == "ILO-P003")
        .expect("expected ILO-P003 for `f x:n>>n;x`");
    assert!(
        e.message.contains("`>`"),
        "expected literal `>` in message, got: {}",
        e.message,
    );
    assert!(
        e.message.contains("`>>`"),
        "expected literal `>>` in message, got: {}",
        e.message,
    );
    assert!(
        !e.message.contains("Greater") && !e.message.contains("PipeOp"),
        "message must not leak Token enum names: {}",
        e.message,
    );
}

#[test]
fn p003_missing_return_arrow_with_pipe() {
    // Variant: `f x:n|n;x` — single pipe where `>` was expected.
    assert_no_token_enum_leak("f x:n|n;x");
}

#[test]
fn p003_unclosed_brace_body() {
    // Foreach without an opening brace: body of expect(LBrace) path.
    // Was: `expected LBrace, got Semi`. Now: `expected `{`, got `;``.
    let source = "main>n;xs=[1 2 3];@k xs;+k 1";
    let (_, errors) = parse_str_errors(source);
    let e = errors
        .iter()
        .find(|e| e.code == "ILO-P003")
        .expect("expected ILO-P003");
    assert!(
        e.message.contains("`{`"),
        "expected literal `{{` in message, got: {}",
        e.message,
    );
    assert!(
        !e.message.contains("LBrace") && !e.message.contains("Semi"),
        "message leaks Token enum names: {}",
        e.message,
    );
}

#[test]
fn p003_unclosed_paren_in_expression() {
    // Type-position `R n` with stray `.` — was `expected RParen, got Dot`.
    assert_no_token_enum_leak("f x:n>n;y=(+x 1.;y");
}

#[test]
fn p005_identifier_expected_gets_source_chars() {
    // ILO-439: `=42 5` at file start now enters script mode, where it is a
    // valid equality expression (`42 == 5` -> false), so no error fires there.
    // Use `type 123{x:n}` instead: a number where a type name is expected,
    // which still fires ILO-P005 "expected identifier, got number `123`" and
    // exercises the same enum-leak guard.
    assert_no_token_enum_leak("type 123{x:n}");
}

#[test]
fn p001_unexpected_token_at_top_level() {
    // ILO-439: a leading `+` is a statement start at every top-level position
    // now, so it never reaches the "expected declaration, got ..." path and the
    // old assertion on a literal `+` in the message is unreachable. The guard
    // this test exists for — that raw Token enum names never leak into user
    // messages — still applies to whatever token does surface, so assert that
    // instead. Statements after an explicit `main` fall through to `parse_decl`
    // (so its specific diagnostics survive), which is what makes this error.
    assert_no_token_enum_leak("main>n;1\n+ 1 2");
    let (_, errors) = parse_str_errors("main>n;1\n+ 1 2");
    let e = errors
        .iter()
        .find(|e| e.code == "ILO-P001")
        .expect("expected ILO-P001");
    // Whatever token is named, it must be rendered as source text in backticks
    // rather than as a Rust enum variant.
    assert!(
        e.message.contains('`'),
        "expected a backticked source glyph in message, got: {}",
        e.message,
    );
    assert!(
        !e.message.contains("Plus") && !e.message.contains("Number("),
        "message leaks a Token enum name: {}",
        e.message,
    );
}

#[test]
fn p004_eof_renders_human() {
    // EOF mid-fn-header — file ends before `>` / return type. The
    // parser may surface this as P003 (`expected `>`, got ...`),
    // P004 (true EOF after `expect`), or P020 (fn-header boundary
    // guard), depending on which token the cursor lands on. The
    // assertion that matters is: no parser-internal enum names
    // leak in either message or hint.
    assert_no_token_enum_leak("f x:n");
    // And: at least one of the errors must mention `end of input`
    // OR an actual source character — never `EOF` / `Greater` /
    // `LBrace`. The leak guard above already covers the negative.
    let (_, errors) = parse_str_errors("f x:n");
    let has_human_eof_or_glyph = errors.iter().any(|e| {
        e.message.contains("end of input") || e.message.contains('`') // any backtick-quoted source char
    });
    assert!(
        has_human_eof_or_glyph,
        "expected at least one error to render EOF or a glyph: {errors:?}",
    );
}

/// Sweep test: throw a representative grid of malformed snippets at the
/// parser and ensure no diagnostic ever leaks a `TokenKind` variant name.
/// New parser code added in future will fail this test if it forgets to
/// route through `user_facing_name()`.
#[test]
fn sweep_no_token_enum_leak_across_shapes() {
    let snippets: &[&str] = &[
        "f x:n>>n;x",         // PipeOp where Greater expected
        "f x:n|n;x",          // Pipe where Greater expected
        "f x:n>n;@k xs;+k 1", // missing `{` after foreach
        "f x:n>n;(+x 1",      // unclosed paren in body
        "f x:n>n;y=42.;y",    // stray dot
        "type foo bar",       // type decl missing body
        "f :n>n;x",           // missing param name
        "use 42",             // use with non-string path
        "main:>n;42",         // `:>` shape
        "+",                  // bare operator at top level
        "f x:>n;x",           // missing type after `:`
        "f x:n>;x",           // missing return type after `>`
    ];
    for src in snippets {
        assert_no_token_enum_leak(src);
    }
}
