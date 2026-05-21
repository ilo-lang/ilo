//! Regression tests for the AST nesting-depth cap (ILO-P103).
//!
//! Borrowed from Zero (rocicorp/mono#6000): any context that compiles
//! untrusted source — `ilo serv`, the bare-positional dispatch — is exposed
//! to deeply nested expressions that can blow the parser stack. These tests
//! pin the cap behaviour:
//!
//! 1. A 1000-deep nested expression is rejected with `ILO-P103` at the
//!    default cap (256).
//! 2. The cap is overridable via `parser::parse_with_max_depth`, mirroring
//!    the `--max-ast-depth` CLI flag exposed on `ilo` and `ilo serv`.
//! 3. The same input fed through the same parse pipeline `ilo serv` uses is
//!    rejected with an `ILO-P103` parse-phase diagnostic, not a stack
//!    overflow.

use ilo::ast::Span;
use ilo::lexer;
use ilo::parser::{self, DEFAULT_MAX_AST_DEPTH};

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

/// Build a `n`-deep nested expression: `main>n;(((...((1))...)))`.
/// Each paren bumps parser depth by 2 (`parse_expr` → `parse_atom`), so at
/// `n = DEFAULT_MAX_AST_DEPTH / 2` and above the cap fires.
fn deeply_nested_source(n: usize) -> String {
    let mut s = String::with_capacity(7 + n * 2 + 1);
    s.push_str("main>n;");
    for _ in 0..n {
        s.push('(');
    }
    s.push('1');
    for _ in 0..n {
        s.push(')');
    }
    s
}

/// Debug parser frames are ~24 KB each; even with the depth cap in place the
/// parser still recurses up to the cap before erroring out, which blows past
/// the 2 MB default test thread stack. Every test here runs on a 32 MB stack
/// so the cap fires logically rather than via SIGSEGV.
fn run_on_fat_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(f)
        .expect("spawn test thread")
        .join()
        .expect("thread panicked");
}

#[test]
fn deep_nest_at_default_cap_triggers_p103() {
    run_on_fat_stack(|| {
        let src = deeply_nested_source(1000);
        let pairs = lex_to_pairs(&src);
        let (_prog, errs) = parser::parse(pairs);
        assert!(
            errs.iter().any(|e| e.code == "ILO-P103"),
            "expected ILO-P103 at default cap, got {:?}",
            errs.iter().map(|e| e.code).collect::<Vec<_>>()
        );
        let p103 = errs
            .iter()
            .find(|e| e.code == "ILO-P103")
            .expect("ILO-P103 present");
        assert!(
            p103.message.contains(&DEFAULT_MAX_AST_DEPTH.to_string()),
            "P103 message should name the cap; got {:?}",
            p103.message
        );
        assert!(
            p103.hint
                .as_deref()
                .unwrap_or("")
                .contains("--max-ast-depth"),
            "P103 hint should point at the override flag; got {:?}",
            p103.hint
        );
    });
}

#[test]
fn deep_nest_under_cap_parses_clean() {
    run_on_fat_stack(|| {
        // 100 parens => depth 200 < 256 default.
        let src = deeply_nested_source(100);
        let pairs = lex_to_pairs(&src);
        let (_prog, errs) = parser::parse(pairs);
        assert!(
            errs.is_empty(),
            "expected clean parse under cap, got {errs:?}"
        );
    });
}

#[test]
fn explicit_override_raises_cap() {
    run_on_fat_stack(|| {
        // 140 parens => depth ~280 > 256 default; under a raised 1024 cap
        // the same program parses clean.
        let src = deeply_nested_source(140);
        let pairs = lex_to_pairs(&src);
        let (_prog, errs_default) = parser::parse(pairs.clone());
        assert!(
            errs_default.iter().any(|e| e.code == "ILO-P103"),
            "140-deep should trip the default cap; got {:?}",
            errs_default.iter().map(|e| e.code).collect::<Vec<_>>()
        );
        let (_prog, errs_raised) = parser::parse_with_max_depth(pairs, 1024);
        assert!(
            errs_raised.is_empty(),
            "expected clean parse under 1024 cap, got {errs_raised:?}"
        );
    });
}

#[test]
fn explicit_override_can_lower_cap() {
    run_on_fat_stack(|| {
        // 30-deep nest (depth ~60) is rejected under a tight cap.
        let src = deeply_nested_source(30);
        let pairs = lex_to_pairs(&src);
        let (_prog, errs) = parser::parse_with_max_depth(pairs, 32);
        assert!(
            errs.iter().any(|e| e.code == "ILO-P103"),
            "expected ILO-P103 under tight cap, got {:?}",
            errs.iter().map(|e| e.code).collect::<Vec<_>>()
        );
    });
}

/// `ilo serv` exposes a JSON-over-stdio surface that compiles arbitrary
/// program text from clients. The depth cap must reject a deep-nest payload
/// before the parser blows the stack. We don't drive the full `serv_cmd`
/// stdio loop here (it owns stdin), but we exercise the same parse pipeline
/// the serv request handler uses and assert the failure mode.
#[test]
fn serv_style_parse_rejects_deep_nest() {
    run_on_fat_stack(|| {
        let src = deeply_nested_source(1000);
        let tokens = lexer::lex(&src).expect("lex");
        let token_spans: Vec<_> = tokens
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
        let (_prog, errs) = parser::parse(token_spans);
        assert!(
            errs.iter().any(|e| e.code == "ILO-P103"),
            "serv parse path must reject deep nest with ILO-P103, got {:?}",
            errs.iter().map(|e| e.code).collect::<Vec<_>>()
        );
    });
}

/// Defensive: a deeply nested *statement* chain (foreach/if etc.) doesn't
/// share the paren path, but it still pumps `parse_stmt` recursively. Confirm
/// the depth cap covers that surface too.
#[test]
fn deep_nest_statement_chain_triggers_p103() {
    run_on_fat_stack(|| {
        let mut src = String::from("main>n;");
        // wh true{wh true{wh true{ ... ; 1 ... }}} — each `wh true{` adds a
        // nested statement level (parse_stmt -> body -> parse_stmt).
        let n = 300;
        for _ in 0..n {
            src.push_str("wh true{");
        }
        src.push('1');
        for _ in 0..n {
            src.push('}');
        }
        let pairs = lex_to_pairs(&src);
        let (_prog, errs) = parser::parse(pairs);
        assert!(
            errs.iter().any(|e| e.code == "ILO-P103"),
            "deep statement chain should trip ILO-P103, got {:?}",
            errs.iter().map(|e| e.code).collect::<Vec<_>>()
        );
    });
}
