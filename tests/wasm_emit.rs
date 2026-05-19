//! WASM backend emit tests (Phase 5 Stage 5d).
//!
//! Compiles a few `.ilo` sources to `.wasm` via the WASM backend and
//! validates each module with `wasmparser`. Doesn't execute — runtime
//! verification is in `wasm_runtime.rs`.

use std::path::PathBuf;

use ilo::backend::wasm::{emit, check_builtin, WasmConfig, WasmTarget};
use ilo::backend::BackendError;

fn lower(src: &str) -> ilo::hir::Program {
    let tokens = ilo::lexer::lex(src).expect("lex");
    let token_spans: Vec<(ilo::lexer::Token, ilo::ast::Span)> = tokens
        .into_iter()
        .map(|(t, r)| (t, ilo::ast::Span { start: r.start, end: r.end }))
        .collect();
    let (program, parse_errors) = ilo::parser::parse(token_spans);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let verify = ilo::verify::verify(&program);
    assert!(verify.errors.is_empty(), "verify errors: {:?}", verify.errors);
    ilo::hir::lower(&program, &verify).expect("hir lower")
}

fn build_wasm(src: &str, target: WasmTarget) -> PathBuf {
    let hir = lower(src);
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = tmp.path().join("out.wasm");
    let cfg = WasmConfig {
        target,
        output_path: out.clone(),
        entry: None,
    };
    emit(&hir, cfg).expect("emit");
    let bytes = std::fs::read(&out).expect("read out");
    // Validate with wasmparser. For the Component target this is a component,
    // for others it's a core module — `Validator::default()` handles both.
    let mut validator = wasmparser::Validator::new();
    validator.validate_all(&bytes).expect("wasmparser validate");
    // Keep the tempdir alive by returning a copy of the bytes path elsewhere.
    // For simplicity we leak the tempdir handle by forgetting it; the OS
    // cleans `/tmp` on its own.
    std::mem::forget(tmp);
    out
}

#[test]
fn emits_wasip1_hello() {
    let src = "hello>t;prnt \"hi\"";
    let path = build_wasm(src, WasmTarget::Wasip1);
    let bytes = std::fs::read(path).expect("read");
    assert!(bytes.starts_with(b"\0asm"), "wasm magic");
    // Tail u32 1 = core wasm version.
    assert_eq!(&bytes[4..8], &[1, 0, 0, 0]);
}

#[test]
fn emits_component_default() {
    let src = "hello>t;prnt \"hi\"";
    let path = build_wasm(src, WasmTarget::Component);
    let bytes = std::fs::read(&path).expect("read");
    // Component header: \0asm + version 0x0d + layer 0x01.
    assert_eq!(&bytes[0..4], b"\0asm");
    // wasm-tools writes the component layer/version pair in the 5th-8th
    // bytes. We don't pin the exact bytes here — wasmparser validation
    // above already confirms it's a valid component.
    let wit = path.with_extension("wit");
    let wit_text = std::fs::read_to_string(wit).expect("read wit");
    assert!(wit_text.contains("world program"));
    assert!(wit_text.contains("export run"));
}

#[test]
fn unknown_unknown_rejects_prnt() {
    let src = "hello>t;prnt \"hi\"";
    let hir = lower(src);
    let tmp = tempfile::tempdir().unwrap();
    let cfg = WasmConfig {
        target: WasmTarget::UnknownUnknown,
        output_path: tmp.path().join("out.wasm"),
        entry: None,
    };
    let err = emit(&hir, cfg).unwrap_err();
    match err {
        BackendError::CodegenFailed { code, message, .. } => {
            assert_eq!(code, "ILO-B201");
            assert!(message.contains("prnt"), "msg: {}", message);
            assert!(message.contains("wasm32-unknown-unknown"));
            assert!(message.contains("hint"));
        }
        other => panic!("expected ILO-B201 CodegenFailed, got {:?}", other),
    }
}

#[test]
fn capability_check_matrix() {
    // prnt is supported on wasi targets, not on unknown-unknown.
    assert!(check_builtin("prnt", WasmTarget::Wasip1).is_ok());
    assert!(check_builtin("prnt", WasmTarget::Component).is_ok());
    assert!(check_builtin("prnt", WasmTarget::UnknownUnknown).is_err());
    // rd needs WASI filesystem — same shape.
    assert!(check_builtin("rd", WasmTarget::Wasip1).is_ok());
    assert!(check_builtin("rd", WasmTarget::UnknownUnknown).is_err());
    // run is unsupported on every wasm target.
    for t in [
        WasmTarget::Wasip1,
        WasmTarget::Wasip2,
        WasmTarget::Component,
        WasmTarget::UnknownUnknown,
    ] {
        assert!(check_builtin("run", t).is_err(), "run on {:?}", t);
    }
    // Pure ops everywhere.
    for t in [
        WasmTarget::Wasip1,
        WasmTarget::Component,
        WasmTarget::UnknownUnknown,
    ] {
        assert!(check_builtin("len", t).is_ok());
        assert!(check_builtin("map", t).is_ok());
    }
}

#[test]
fn target_parse_accepts_aliases() {
    assert_eq!(WasmTarget::parse("wasm32-wasi"), Some(WasmTarget::Wasip1));
    assert_eq!(WasmTarget::parse("wasm32-wasip1"), Some(WasmTarget::Wasip1));
    assert_eq!(WasmTarget::parse("wasm32-component"), Some(WasmTarget::Component));
    assert_eq!(WasmTarget::parse("wasm32-web"), Some(WasmTarget::UnknownUnknown));
    assert_eq!(WasmTarget::parse("wasm32-unknown-unknown"), Some(WasmTarget::UnknownUnknown));
    assert!(WasmTarget::parse("x86_64-linux-gnu").is_none());
}

#[test]
fn capability_error_json_round_trip() {
    let err = check_builtin("rd", WasmTarget::UnknownUnknown).unwrap_err();
    let json = err.to_json();
    assert_eq!(json["kind"], "codegen_failed");
    assert_eq!(json["code"], "ILO-B201");
    assert!(json["message"].as_str().unwrap().contains("rd"));
}
