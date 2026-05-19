//! WASM runtime integration test (Phase 5 Stage 5d).
//!
//! Compiles an ilo program to wasm and runs it on Wasmtime, asserting
//! stdout matches what the tree interpreter would produce.
//!
//! Skipped automatically when `wasmtime` is not on PATH so the test suite
//! still runs cleanly in environments that lack the runtime.

use std::path::PathBuf;
use std::process::Command;

use ilo::backend::wasm::{emit, WasmConfig, WasmTarget};

fn wasmtime_available() -> bool {
    Command::new("wasmtime").arg("--version").output().is_ok()
}

fn build(src: &str, target: WasmTarget, dir: &std::path::Path) -> PathBuf {
    let tokens = ilo::lexer::lex(src).expect("lex");
    let token_spans: Vec<(ilo::lexer::Token, ilo::ast::Span)> = tokens
        .into_iter()
        .map(|(t, r)| (t, ilo::ast::Span { start: r.start, end: r.end }))
        .collect();
    let (program, parse_errors) = ilo::parser::parse(token_spans);
    assert!(parse_errors.is_empty(), "parse: {:?}", parse_errors);
    let verify = ilo::verify::verify(&program);
    assert!(verify.errors.is_empty(), "verify: {:?}", verify.errors);
    let hir = ilo::hir::lower(&program, &verify).expect("lower");
    let out = dir.join("hello.wasm");
    let cfg = WasmConfig {
        target,
        output_path: out.clone(),
        entry: None,
    };
    emit(&hir, cfg).expect("emit");
    out
}

#[test]
fn wasip1_hello_runs_on_wasmtime() {
    if !wasmtime_available() {
        eprintln!("skip: wasmtime not on PATH");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = build("hello>t;prnt \"Hello, WASM!\"", WasmTarget::Wasip1, dir.path());
    let out = Command::new("wasmtime").arg(&path).output().expect("wasmtime run");
    assert!(
        out.status.success(),
        "wasmtime exit: {} stderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim_end(), "Hello, WASM!");
}

#[test]
fn wasip1_multiple_prints() {
    if !wasmtime_available() {
        eprintln!("skip: wasmtime not on PATH");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let src = "go>t;prnt \"one\";prnt \"two\";prnt \"three\"";
    let path = build(src, WasmTarget::Wasip1, dir.path());
    let out = Command::new("wasmtime").arg(&path).output().expect("wasmtime");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines, vec!["one", "two", "three"]);
}
