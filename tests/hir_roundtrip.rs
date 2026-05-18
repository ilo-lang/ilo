//! HIR round-trip test.
//!
//! For every `examples/*.ilo` file that has annotated `-- run: <fn>` lines,
//! parse + verify + desugar the program to an AST, then:
//!
//!   1. Walk the AST directly via the existing tree interpreter → output A.
//!   2. Lower AST → HIR, raise HIR → AST', walk AST' via the tree
//!      interpreter → output B.
//!   3. Assert A == B (same Value or same RuntimeError shape).
//!
//! This proves the lowering pass preserves enough information to reconstruct
//! a semantically equivalent program.
//!
//! Only `-- run:` lines with **no** arguments are exercised (arg parsing
//! lives in `main.rs` and isn't exposed as library API). That still gives a
//! solid corpus of ~350 no-arg entry-point invocations across `examples/`.
//! Stage 5b will deepen this when the proper Backend trait lands and we can
//! drive the conformance fixtures against the HIR directly.

use std::path::PathBuf;

use ilo::ast;
use ilo::hir;
use ilo::interpreter;
use ilo::lexer;
use ilo::parser;
use ilo::verify;

fn find_examples() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read examples/ at {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "ilo").unwrap_or(false))
        .collect();
    paths.sort();
    paths
}

/// Parsed `-- run:` entries that we exercise. We deliberately only collect
/// the no-arg cases — see file header.
struct NoArgCase {
    func: String,
    line: usize,
}

fn parse_no_arg_cases(src: &str) -> Vec<NoArgCase> {
    let mut cases = Vec::new();
    for (i, raw) in src.lines().enumerate() {
        let line = raw.trim();
        let Some(rest) = line.strip_prefix("-- run:") else {
            continue;
        };
        let parts: Vec<&str> = rest.split_whitespace().collect();
        // Exactly one token = function name with no args.
        if parts.len() == 1 {
            cases.push(NoArgCase {
                func: parts[0].to_string(),
                line: i + 1,
            });
        }
    }
    cases
}

/// Parse + verify + apply the same desugarings that `main.rs` applies before
/// dispatch. Returns `None` if the program fails to lex/parse/verify (those
/// examples are intentionally invalid and not part of the round-trip
/// corpus).
fn parse_and_verify(source: &str) -> Option<ast::Program> {
    let raw_tokens = lexer::lex(source).ok()?;
    let tokens: Vec<(lexer::Token, ast::Span)> = raw_tokens
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

    let (mut program, parse_errors) = parser::parse(tokens);
    if !parse_errors.is_empty() {
        return None;
    }

    ast::resolve_aliases(&mut program);
    ast::desugar_dot_var_index(&mut program);
    program.source = Some(source.to_string());

    let vr = verify::verify(&program);
    if !vr.errors.is_empty() {
        return None;
    }
    Some(program)
}

/// Stringify the result of `interpreter::run` so we can compare A and B
/// without depending on `Value: Eq` for every internal shape (closures
/// contain captured values; comparing them via Display is robust enough).
fn outcome_string(r: Result<interpreter::Value, interpreter::RuntimeError>) -> String {
    match r {
        Ok(v) => format!("ok:{v}"),
        Err(e) => format!("err:{}:{}", e.code, e.message),
    }
}

#[test]
fn hir_roundtrip_matches_ast() {
    let files = find_examples();
    assert!(!files.is_empty(), "no .ilo files found in examples/");

    // Examples that the round-trip currently doesn't reach. Empty today —
    // every file that parses + verifies and has a no-arg `-- run:` line
    // round-trips cleanly. Listed here as a single source of truth so
    // additions get noticed.
    let skip: &[&str] = &[];

    let mut total = 0;
    let mut skipped_unparseable = 0;
    let mut skipped_no_cases = 0;
    let mut failures: Vec<String> = Vec::new();

    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if skip.contains(&name.as_str()) {
            continue;
        }

        let Ok(src) = std::fs::read_to_string(path) else {
            continue;
        };

        let cases = parse_no_arg_cases(&src);
        if cases.is_empty() {
            skipped_no_cases += 1;
            continue;
        }

        let Some(program) = parse_and_verify(&src) else {
            // Many examples are intentionally bad (negative tests). Skip
            // and don't count as a corpus failure.
            skipped_unparseable += 1;
            continue;
        };

        // Lower → raise to produce the round-trip AST.
        let vr = verify::verify(&program);
        let hir_prog = match hir::lower(&program, &vr) {
            Ok(h) => h,
            Err(e) => {
                failures.push(format!("{name}: hir::lower failed: {e}"));
                continue;
            }
        };
        let raised = hir::raise::raise(&hir_prog);

        for case in &cases {
            total += 1;
            let a = outcome_string(interpreter::run(&program, Some(&case.func), vec![]));
            let b = outcome_string(interpreter::run(&raised, Some(&case.func), vec![]));
            if a != b {
                failures.push(format!(
                    "{name}:{} fn {}\n  ast:  {a}\n  hir:  {b}",
                    case.line, case.func,
                ));
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "{} round-trip mismatches out of {total} (skipped {skipped_unparseable} unparseable, {skipped_no_cases} no no-arg cases):\n\n{}",
            failures.len(),
            failures.join("\n\n"),
        );
    }

    println!(
        "HIR round-trip: {total} cases passed across {} files (skipped {skipped_unparseable} unparseable, {skipped_no_cases} files with no no-arg run lines)",
        files.len(),
    );
}

#[test]
fn hir_lower_drops_alias_use_decls() {
    // Verify the documented lowering: Alias and Use decls disappear in HIR.
    // We construct the AST directly so the test doesn't depend on parser
    // surface syntax for `alias`.
    let prog = ast::Program {
        declarations: vec![
            ast::Decl::Alias {
                name: "N".to_string(),
                target: ast::Type::Number,
                span: ast::Span::UNKNOWN,
            },
            ast::Decl::Function {
                name: "greet".to_string(),
                params: vec![],
                return_type: ast::Type::Text,
                body: vec![ast::Spanned::unknown(ast::Stmt::Expr(ast::Expr::Literal(
                    ast::Literal::Text("hi".to_string()),
                )))],
                span: ast::Span::UNKNOWN,
            },
        ],
        source: None,
    };
    let vr = verify::verify(&prog);
    let h = hir::lower(&prog, &vr).expect("lower");
    let names: Vec<&str> = h
        .decls
        .iter()
        .map(|d| match d {
            hir::Decl::Function { name, .. } => name.as_str(),
            hir::Decl::TypeDef { name, .. } => name.as_str(),
            hir::Decl::Tool { name, .. } => name.as_str(),
        })
        .collect();
    assert_eq!(names, vec!["greet"], "alias should be dropped");
}

#[test]
fn hir_lower_splits_body_tail() {
    // A function whose last statement is a bare expression should produce
    // a body with that expression in `tail`, not `stmts`.
    let src = r#"
inc x:n>n
+x 1
"#;
    let prog = parse_and_verify(src).expect("parse+verify");
    let vr = verify::verify(&prog);
    let h = hir::lower(&prog, &vr).expect("lower");
    let body = match &h.decls[0] {
        hir::Decl::Function { body, .. } => body,
        _ => panic!("expected function decl"),
    };
    assert!(body.tail.is_some(), "trailing expression should be in tail");
    assert!(body.stmts.is_empty(), "no prefix statements expected");
}

#[test]
fn hir_lower_folds_negated_guard() {
    // `!cond { body }` should land in HIR as `If { cond: !cond, ... }` with
    // the negation folded onto the condition (never `negated: true` flag).
    // We construct the AST by hand so the test doesn't drift if guard
    // parser syntax changes.
    let cond = ast::Expr::Literal(ast::Literal::Bool(true));
    let prog = ast::Program {
        declarations: vec![ast::Decl::Function {
            name: "go".to_string(),
            params: vec![],
            return_type: ast::Type::Number,
            body: vec![
                ast::Spanned::unknown(ast::Stmt::Guard {
                    condition: cond,
                    negated: true,
                    body: vec![ast::Spanned::unknown(ast::Stmt::Return(
                        ast::Expr::Literal(ast::Literal::Number(1.0)),
                    ))],
                    else_body: None,
                    braceless: false,
                }),
                ast::Spanned::unknown(ast::Stmt::Expr(ast::Expr::Literal(ast::Literal::Number(
                    2.0,
                )))),
            ],
            span: ast::Span::UNKNOWN,
        }],
        source: None,
    };
    let vr = verify::verify(&prog);
    let h = hir::lower(&prog, &vr).expect("lower");
    let body = match &h.decls[0] {
        hir::Decl::Function { body, .. } => body,
        _ => panic!("expected function decl"),
    };
    let has_unary_not_in_if = body.stmts.iter().any(|s| {
        if let hir::Stmt::If { cond, .. } = s {
            matches!(
                cond,
                hir::Expr::UnaryOp {
                    op: ast::UnaryOp::Not,
                    ..
                }
            )
        } else {
            false
        }
    });
    assert!(
        has_unary_not_in_if,
        "negated guard should fold into UnaryOp(Not) on the If condition"
    );
}
