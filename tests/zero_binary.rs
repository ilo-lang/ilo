//! Zero backend binary integration test (Phase 5 Stage 5e).
//!
//! Build an ilo program via `--0bin` (which shells out to the pinned
//! `zero` compiler) and verify the resulting native binary produces the
//! same stdout as direct ilo execution.
//!
//! Skipped automatically when `zero` is not on PATH.

use std::path::Path;
use std::process::Command;

use ilo::backend::zero::{ZeroConfig, ZeroMode, default_zero_path, emit};

fn zero_available() -> bool {
    if let Some(p) = default_zero_path() {
        if p.is_file() {
            return true;
        }
    }
    Command::new("zero")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

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

fn build_binary(src: &str, dir: &Path) -> std::path::PathBuf {
    let hir = lower(src);
    let out = dir.join("prog");
    let cfg = ZeroConfig {
        output_path: out.clone(),
        mode: ZeroMode::Binary,
        entry: None,
    };
    emit(&hir, cfg).expect("emit");
    out
}

#[test]
fn round_trip_hello_world() {
    if !zero_available() {
        eprintln!("skip: zero not on PATH");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let bin = build_binary("hello>t;prnt \"Hello, Zero!\"", dir.path());
    let out = Command::new(&bin).output().expect("run binary");
    assert!(
        out.status.success(),
        "binary exit: {} stderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim_end(),
        "Hello, Zero!"
    );
}

#[test]
fn round_trip_multi_print_matches_tree_interpreter() {
    if !zero_available() {
        eprintln!("skip: zero not on PATH");
        return;
    }
    let src = "hello>t;prnt \"alpha\";prnt \"beta\";prnt \"gamma\"";
    let dir = tempfile::tempdir().unwrap();
    let bin = build_binary(src, dir.path());
    let out = Command::new(&bin).output().expect("run binary");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        stdout.lines().collect::<Vec<_>>(),
        vec!["alpha", "beta", "gamma"]
    );
}
