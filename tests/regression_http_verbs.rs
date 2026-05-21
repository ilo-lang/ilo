// Cross-engine regression tests for the HTTP verb cluster (#5z):
// `put`, `pat`, `del`, `hed`, `opt`.
//
// Each verb mirrors the shape of `pst`/`get`:
//   - PUT/PAT take (url, body) with an optional `M t t` headers map.
//   - DEL/HED/OPT take (url) with an optional `M t t` headers map.
// All return `R t t` and route through the tree-bridge on VM and JIT, so
// behaviour must be identical across engines.
//
// We cannot dial real hosts in CI, so the network behavioural check is a
// bad-host pattern: any unreachable hostname must surface as `Err`, never
// panic, on every engine. The verifier-level smoke tests live in
// `coverage_verify.rs`; this file is the engine-level companion.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--vm"];

const BAD_HOST: &str = "http://ilo-lang-test-nonexistent.invalid/endpoint";

fn run(engine: &str, src: &str, entry: &str, args: &[&str]) -> (bool, String, String) {
    let mut cmd = ilo();
    cmd.args([src, engine, entry]);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Asserts that an HTTP-verb call surfaces as a Result `Err` on every engine
/// — never as a panic, abort, or success. The CLI prints the Err with a `^`
/// prefix on stderr and exits non-zero, so we check both channels rather than
/// `status.success()`.
fn assert_err_on_all_engines(name: &str, src: &str, args: &[&str]) {
    for engine in ENGINES_ALL {
        let (_ok, stdout, stderr) = run(engine, src, "f", args);
        let combined = format!("{stdout}{stderr}");
        assert!(
            combined.contains('^') || stderr.contains("Err"),
            "{engine}: {name} bad-host should surface as Err; got stdout={stdout:?} stderr={stderr:?}"
        );
        // The "panic-grade" failure modes we're guarding against — segfault,
        // assertion failure, unreachable! — produce a "panicked at" line.
        assert!(
            !combined.contains("panicked at"),
            "{engine}: {name} bad-host panicked: {combined}"
        );
    }
}

// --- bad-host: each verb must return Err on every engine, never panic ---

#[test]
fn put_bad_host_cross_engine() {
    assert_err_on_all_engines("put", "f u:t b:t>R t t;put u b", &[BAD_HOST, "{}"]);
}

#[test]
fn pat_bad_host_cross_engine() {
    assert_err_on_all_engines("pat", "f u:t b:t>R t t;pat u b", &[BAD_HOST, "{}"]);
}

#[test]
fn del_bad_host_cross_engine() {
    assert_err_on_all_engines("del", "f u:t>R t t;del u", &[BAD_HOST]);
}

#[test]
fn hed_bad_host_cross_engine() {
    assert_err_on_all_engines("hed", "f u:t>R t t;hed u", &[BAD_HOST]);
}

#[test]
fn opt_bad_host_cross_engine() {
    assert_err_on_all_engines("opt", "f u:t>R t t;opt u", &[BAD_HOST]);
}

// --- headers variant: must verify and route through the bridge cleanly ---

#[test]
fn put_with_headers_bad_host_cross_engine() {
    // Build a headers map and confirm the 3-arg PUT also returns Err.
    assert_err_on_all_engines(
        "put+headers",
        "f u:t b:t>R t t;h=mmap;h=mset h \"x-api-key\" \"k\";put u b h",
        &[BAD_HOST, "{}"],
    );
}

#[test]
fn del_with_headers_bad_host_cross_engine() {
    assert_err_on_all_engines(
        "del+headers",
        "f u:t>R t t;h=mmap;h=mset h \"x-api-key\" \"k\";del u h",
        &[BAD_HOST],
    );
}

// --- verify-only: every verb parses and type-checks against R t t ---

#[test]
fn http_verbs_parse_and_verify() {
    let cases = [
        ("put", r#"f u:t b:t>R t t;put u b"#),
        ("pat", r#"f u:t b:t>R t t;pat u b"#),
        ("del", r#"f u:t>R t t;del u"#),
        ("hed", r#"f u:t>R t t;hed u"#),
        ("opt", r#"f u:t>R t t;opt u"#),
    ];
    for (name, src) in cases {
        let out = ilo()
            .args(["--ast", src])
            .output()
            .expect("failed to run ilo");
        assert!(
            out.status.success(),
            "{name} should verify cleanly; stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

// --- bang-form: `verb! url body` (or `verb! url`) must auto-unwrap on a
//     Result-returning caller via the tree-bridge `tree_bridge_returns_result`
//     allow-list. Compiles + verifies; we don't dial the network here.

#[test]
fn http_verbs_bang_form_verifies() {
    let cases = [
        (r#"f u:t b:t>R t t;v=put! u b;~v"#),
        (r#"f u:t b:t>R t t;v=pat! u b;~v"#),
        (r#"f u:t>R t t;v=del! u;~v"#),
        (r#"f u:t>R t t;v=hed! u;~v"#),
        (r#"f u:t>R t t;v=opt! u;~v"#),
    ];
    for src in cases {
        let out = ilo()
            .args(["--ast", src])
            .output()
            .expect("failed to run ilo");
        assert!(
            out.status.success(),
            "bang-form {src:?} should verify cleanly; stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
