// Regression tests for multi-line function bodies.
//
// Previously the parser required a literal `;` between the function
// header (`name params > return_type`) and the body. When the body sat
// on a separate, unindented line — which `normalize_newlines` filters
// out at lex time — the parser saw the body token directly after the
// return type and erroneously tried to read more of the type or fell
// through to a cryptic "expected Semi" / "expected expression, got Semi"
// error. This was especially common with multi-token return types like
// `R t t` and `L n`, where agents naturally wanted to put the header on
// its own line.
//
// The parser now treats the header/body boundary as either a `;` or
// nothing (a newline), so all four combinations work uniformly:
//   `f>n;5`           single-line, simple return type
//   `f>R t t;~"hi"`   single-line, multi-token return type
//   `f>n\n5`          multi-line, simple return type
//   `f>R t t\n~"hi"`  multi-line, multi-token return type

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_file(engine: &str, src: &str, entry: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "ilo_multiline_fn_{}_{}.ilo",
        std::process::id(),
        seq
    ));
    std::fs::write(&path, src).unwrap();
    let out = ilo()
        .args([path.to_str().unwrap(), engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// Multi-line body, multi-token return type `R t t`.
const ML_RESULT: &str = "f>R t t\n~\"hello\"\n";
// Multi-line body, list return type `L n`.
const ML_LIST: &str = "g>L n\n[1,2,3]\n";
// Multi-line body, three-token return type — exercises a longer type
// run after `>` than `R t t` to make sure the boundary logic doesn't
// miscount tokens.
const ML_NESTED: &str = "h>R L n t\n~[1,2]\n";
// Indented body (already worked, ensure no regression).
const ML_INDENTED: &str = "f>R t t\n  ~\"hello\"\n";
// Single-line baseline, simple return type.
const SL_SIMPLE: &str = "f>n;5\n";
// Single-line baseline, multi-token return type.
const SL_RESULT: &str = "f>R t t;~\"hello\"\n";

fn check_all(engine: &str) {
    assert_eq!(
        run_file(engine, ML_RESULT, "f"),
        "~hello",
        "multi-line R t t engine={engine}"
    );
    assert_eq!(
        run_file(engine, ML_LIST, "g"),
        "[1, 2, 3]",
        "multi-line L n engine={engine}"
    );
    assert_eq!(
        run_file(engine, ML_NESTED, "h"),
        "~[1, 2]",
        "multi-line R L n t engine={engine}"
    );
    assert_eq!(
        run_file(engine, ML_INDENTED, "f"),
        "~hello",
        "multi-line indented engine={engine}"
    );
    assert_eq!(
        run_file(engine, SL_SIMPLE, "f"),
        "5",
        "single-line simple engine={engine}"
    );
    assert_eq!(
        run_file(engine, SL_RESULT, "f"),
        "~hello",
        "single-line R t t engine={engine}"
    );
}

#[test]
fn multiline_fn_body_tree() {
    check_all("--run-tree");
}

#[test]
fn multiline_fn_body_vm() {
    check_all("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn multiline_fn_body_cranelift() {
    check_all("--run-cranelift");
}
