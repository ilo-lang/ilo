//! Zero backend emit tests (Phase 5 Stage 5e).
//!
//! Compile a small corpus of `.ilo` programs to `.0` Zero source and check
//! each one with the pinned `zero check` subprocess. Skipped automatically
//! when `zero` is not on PATH so CI environments without the toolchain
//! still run cleanly.

use std::path::PathBuf;
use std::process::Command;

use ilo::backend::zero::{emit, ZeroConfig, ZeroMode, DEFAULT_ZERO_PATH};

fn zero_bin() -> Option<String> {
    if std::path::Path::new(DEFAULT_ZERO_PATH).is_file() {
        return Some(DEFAULT_ZERO_PATH.to_string());
    }
    if Command::new("zero").arg("--version").output().is_ok() {
        return Some("zero".to_string());
    }
    None
}

fn lower(src: &str) -> ilo::hir::Program {
    let tokens = ilo::lexer::lex(src).expect("lex");
    let token_spans: Vec<(ilo::lexer::Token, ilo::ast::Span)> = tokens
        .into_iter()
        .map(|(t, r)| (t, ilo::ast::Span { start: r.start, end: r.end }))
        .collect();
    let (program, parse_errors) = ilo::parser::parse(token_spans);
    assert!(parse_errors.is_empty(), "parse: {:?}", parse_errors);
    let verify = ilo::verify::verify(&program);
    assert!(verify.errors.is_empty(), "verify: {:?}", verify.errors);
    ilo::hir::lower(&program, &verify).expect("lower")
}

fn build_zero(src: &str) -> (PathBuf, String) {
    let hir = lower(src);
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path().join("out.0");
    let cfg = ZeroConfig {
        output_path: out.clone(),
        mode: ZeroMode::Source,
        entry: None,
    };
    emit(&hir, cfg).expect("emit");
    let text = std::fs::read_to_string(&out).expect("read");
    std::mem::forget(tmp);
    (out, text)
}

#[test]
fn emits_idiomatic_main_shape() {
    let (_, text) = build_zero("hello>t;prnt \"hi\"");
    assert!(
        text.contains("pub fun main(world: World) -> Void raises {"),
        "got: {}",
        text
    );
    assert!(
        text.contains("check world.out.write(\"hi\\n\")"),
        "got: {}",
        text
    );
}

#[test]
fn zero_check_accepts_hello_world() {
    let Some(zero) = zero_bin() else {
        eprintln!("skip: zero not on PATH");
        return;
    };
    let (path, _) = build_zero("hello>t;prnt \"hello\"");
    let out = Command::new(&zero).arg("check").arg(&path).output().expect("zero check");
    assert!(
        out.status.success(),
        "zero check failed: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn zero_check_accepts_multi_print() {
    let Some(zero) = zero_bin() else {
        eprintln!("skip: zero not on PATH");
        return;
    };
    let (path, text) = build_zero("hello>t;prnt \"a\";prnt \"b\";prnt \"c\"");
    // Three write calls in order.
    let a = text.find("\"a\\n\"").expect("a");
    let b = text.find("\"b\\n\"").expect("b");
    let c = text.find("\"c\\n\"").expect("c");
    assert!(a < b && b < c, "order preserved: {}", text);
    let out = Command::new(&zero).arg("check").arg(&path).output().expect("zero check");
    assert!(
        out.status.success(),
        "zero check failed: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn emit_to_string_renders_directly() {
    let s = ilo::backend::zero::emit_to_string(&["hello".to_string()]);
    assert!(s.contains("pub fun main(world: World) -> Void raises {"));
    assert!(s.contains("\"hello\\n\""));
}
