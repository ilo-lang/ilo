// Cross-engine regression tests for `jpar!` — JSON parse with `!`
// auto-unwrap.
//
// `jpar s` returns `R _ t`; the `!` suffix is the generic auto-unwrap
// operator on any Result-returning call. There is no dedicated
// `JparBang` builtin variant — `!` is parsed/lowered uniformly across
// builtins — but `jpar!` is the single most-cited shape in persona
// dogfooding so it earns its own dedicated cross-engine assertions
// distinct from the existing incidental coverage in
// coverage_vm_mod / coverage_cranelift_compile / regression_dot_keywords.
//
// Contract under test:
//   1. `jpar! good` returns the parsed value (field access works).
//   2. `jpar! bad` propagates `^e` out of the enclosing R-returning fn.
//   3. Same byte-identical answer on every engine that ships in this
//      build (VM always, JIT when `cranelift` is enabled).

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn engines() -> Vec<&'static str> {
    let mut v = vec!["--vm"];
    if cfg!(feature = "cranelift") {
        v.push("--jit");
    }
    v
}

fn run_ok(engine: &str, src: &str, entry: &str, arg: &str) -> String {
    let out = ilo()
        .arg(src)
        .arg(engine)
        .arg(entry)
        .arg(arg)
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for entry={entry} arg={arg}: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(engine: &str, src: &str, entry: &str, arg: &str) -> String {
    let out = ilo()
        .arg(src)
        .arg(engine)
        .arg(entry)
        .arg(arg)
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} unexpectedly succeeded for entry={entry} arg={arg}: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).trim().to_string()
}

// Ok path: jpar! unwraps inside an R-returning fn and field access
// works on the resulting json record.
const GOOD: &str = "f body:t>R t t;r=jpar! body;~r.name";

#[test]
fn jpar_bang_ok_unwraps_to_record() {
    for engine in engines() {
        let got = run_ok(engine, GOOD, "f", "{\"name\":\"alice\"}");
        assert_eq!(got, "alice", "engine={engine}");
    }
}

// Err path: malformed JSON makes jpar! propagate the parse error as
// the enclosing function's return value. The CLI surfaces a propagated
// `^e` as a non-zero exit with the error text on stderr.
const BAD: &str = "f body:t>R t t;r=jpar! body;~r.name";

#[test]
fn jpar_bang_err_propagates_parse_error() {
    for engine in engines() {
        let stderr = run_err(engine, BAD, "f", "not-json");
        assert!(
            stderr.contains("expected ident")
                || stderr.contains("expected")
                || stderr.contains("parse"),
            "engine={engine}: stderr did not look like a parse error: {stderr:?}"
        );
    }
}

// `jpar!` on an array body produces a list that `len` can measure.
// Mirrors the persona use-case where the response shape is known and
// the agent wants to skip the Result handshake.
const LIST_LEN: &str = "f body:t>R n t;xs=jpar-list! body;~len xs";

#[test]
fn jpar_list_bang_iterates_top_level_array() {
    for engine in engines() {
        let got = run_ok(engine, LIST_LEN, "f", "[\"a\",\"b\",\"c\"]");
        assert_eq!(got, "3", "engine={engine}");
    }
}
