//! Zero backend capability tests (Phase 5 Stage 5e).
//!
//! Asserts unsupported HIR shapes surface as `BackendError::CodegenFailed`
//! with the documented `ILO-B3##` error codes.

use ilo::backend::BackendError;
use ilo::backend::zero::{ZeroConfig, ZeroMode, emit};

fn lower(src: &str) -> ilo::hir::Program {
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
    let (program, parse_errors) = ilo::parser::parse(token_spans);
    assert!(parse_errors.is_empty(), "parse: {:?}", parse_errors);
    let verify = ilo::verify::verify(&program);
    assert!(verify.errors.is_empty(), "verify: {:?}", verify.errors);
    ilo::hir::lower(&program, &verify).expect("lower")
}

fn try_emit(src: &str) -> Result<(), BackendError> {
    let hir = lower(src);
    let tmp = tempfile::tempdir().unwrap();
    let cfg = ZeroConfig {
        output_path: tmp.path().join("out.0"),
        mode: ZeroMode::Source,
        entry: None,
    };
    emit(&hir, cfg).map(|_| ())
}

#[test]
fn non_prnt_call_errors_with_b302() {
    // `now` returns a number, but with `>n` it type-checks. The Stage 5e
    // walker only knows `prnt`, so any other call surfaces as ILO-B302.
    let err = try_emit("f>n;now").unwrap_err();
    match err {
        BackendError::CodegenFailed { code, message, .. } => {
            assert_eq!(code, "ILO-B302");
            assert!(
                message.contains("now") || message.contains("call"),
                "msg: {}",
                message
            );
            assert!(message.contains("hint"), "msg: {}", message);
        }
        other => panic!("expected ILO-B302, got {:?}", other),
    }
}

#[test]
fn let_binding_errors_with_b302() {
    // A `let` statement triggers the "non-expression statement" branch.
    let err = try_emit("f>t;x=1;prnt \"hi\"").unwrap_err();
    match err {
        BackendError::CodegenFailed { code, .. } => {
            assert_eq!(code, "ILO-B302");
        }
        other => panic!("expected ILO-B302, got {:?}", other),
    }
}

#[test]
fn missing_entry_errors_with_b305() {
    let hir = lower("hello>t;prnt \"hi\"");
    let tmp = tempfile::tempdir().unwrap();
    let cfg = ZeroConfig {
        output_path: tmp.path().join("out.0"),
        mode: ZeroMode::Source,
        entry: Some("does_not_exist".to_string()),
    };
    let err = emit(&hir, cfg).unwrap_err();
    match err {
        BackendError::CodegenFailed { code, message, .. } => {
            assert_eq!(code, "ILO-B305");
            assert!(message.contains("does_not_exist"));
        }
        other => panic!("expected ILO-B305, got {:?}", other),
    }
}

#[test]
fn capability_error_json_round_trip() {
    let err = try_emit("f>n;now").unwrap_err();
    let json = err.to_json();
    assert_eq!(json["kind"], "codegen_failed");
    assert_eq!(json["code"], "ILO-B302");
}

#[test]
fn supported_hello_world_emits_clean() {
    try_emit("hello>t;prnt \"hi\"").expect("supported");
}
