// Regression tests for leading-uppercase JSON keys at dot-access position.
//
// PR #220 added camelCase tail absorption (`r.baseSeverity`, `r.gitURL`) by
// scanning forward through `[A-Za-z0-9]+` and merging into the preceding
// `Ident`. That fix doesn't help when the *first* character after `.` is
// uppercase — there's no preceding Ident to merge into, so `r.URL` and
// `r.ID` still trip the lexer.
//
// Real-world JSON keys are commonly leading-uppercase: AWS uses `AccessKey`,
// `SecretAccessKey`, `URL`, `ID` everywhere, .NET conventions surface
// `PascalCase` keys across many APIs. This fix extends the post-dot pass to
// emit a fresh `Ident` token covering the whole identifier-shaped run when
// the previous token is `Dot`/`DotQuestion` flush against the offending
// uppercase byte.
//
// The strict lowercase rule on bindings is preserved.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine).arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(src: &str) -> String {
    let out = ilo()
        .args([src, "--run-tree", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success(), "expected failure for `{src}`");
    String::from_utf8_lossy(&out.stderr).to_string()
}

// `r.URL` — `U` is a non-sigil uppercase letter (logos rejects it). The
// previous token is `Dot` flush, so the lexer emits a fresh `Ident("URL")`.
const URL: &str = "f j:t>R n t;r=jpar! j;r.URL";

fn check_url(engine: &str) {
    assert_eq!(
        run(engine, URL, "f", &[r#"{"URL":"https://example.com"}"#]),
        "https://example.com",
        "engine={engine}"
    );
}

#[test]
fn leading_upper_url_tree() {
    check_url("--run-tree");
}

#[test]
fn leading_upper_url_vm() {
    check_url("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn leading_upper_url_cranelift() {
    check_url("--run-cranelift");
}

// `r.ID` — `I` is a non-sigil uppercase letter, two letters total.
const ID: &str = "f j:t>R n t;r=jpar! j;r.ID";

fn check_id(engine: &str) {
    assert_eq!(
        run(engine, ID, "f", &[r#"{"ID":42}"#]),
        "42",
        "engine={engine}"
    );
}

#[test]
fn leading_upper_id_tree() {
    check_id("--run-tree");
}

#[test]
fn leading_upper_id_vm() {
    check_id("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn leading_upper_id_cranelift() {
    check_id("--run-cranelift");
}

// `r.AccessKey` — leading uppercase + mixed-case tail (PascalCase). `A` is
// not a sigil; the run `AccessKey` becomes a single Ident.
const ACCESS_KEY: &str = "f j:t>R n t;r=jpar! j;r.AccessKey";

fn check_access_key(engine: &str) {
    assert_eq!(
        run(engine, ACCESS_KEY, "f", &[r#"{"AccessKey":"abc123"}"#]),
        "abc123",
        "engine={engine}"
    );
}

#[test]
fn leading_upper_access_key_tree() {
    check_access_key("--run-tree");
}

#[test]
fn leading_upper_access_key_vm() {
    check_access_key("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn leading_upper_access_key_cranelift() {
    check_access_key("--run-cranelift");
}

// `r.?URL` — safe-access form. Same path, prev token is `DotQuestion`.
const SAFE_URL: &str = "f j:t>R n t;r=jpar! j;r.?URL";

fn check_safe_url_present(engine: &str) {
    assert_eq!(
        run(engine, SAFE_URL, "f", &[r#"{"URL":"https://x"}"#]),
        "https://x",
        "engine={engine}"
    );
}

fn check_safe_url_missing(engine: &str) {
    assert_eq!(
        run(engine, SAFE_URL, "f", &[r#"{"other":1}"#]),
        "nil",
        "engine={engine}"
    );
}

#[test]
fn leading_upper_safe_url_present_tree() {
    check_safe_url_present("--run-tree");
}

#[test]
fn leading_upper_safe_url_present_vm() {
    check_safe_url_present("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn leading_upper_safe_url_present_cranelift() {
    check_safe_url_present("--run-cranelift");
}

#[test]
fn leading_upper_safe_url_missing_tree() {
    check_safe_url_missing("--run-tree");
}

#[test]
fn leading_upper_safe_url_missing_vm() {
    check_safe_url_missing("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn leading_upper_safe_url_missing_cranelift() {
    check_safe_url_missing("--run-cranelift");
}

// Mixed camel + snake: `r.URL_count`. The leading-uppercase pass emits a
// fresh `Ident("URL")` inside the main lex loop, then the post-lex snake
// pass stitches `_count` onto the end.
const URL_COUNT: &str = "f j:t>R n t;r=jpar! j;r.URL_count";

fn check_url_count(engine: &str) {
    assert_eq!(
        run(engine, URL_COUNT, "f", &[r#"{"URL_count":7}"#]),
        "7",
        "engine={engine}"
    );
}

#[test]
fn leading_upper_url_count_tree() {
    check_url_count("--run-tree");
}

#[test]
fn leading_upper_url_count_vm() {
    check_url_count("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn leading_upper_url_count_cranelift() {
    check_url_count("--run-cranelift");
}

// Leading-uppercase that happens to be a type sigil: `r.MetaData` (`M` is
// the MapType sigil) and `r.LeftValue` (`L` is the ListType sigil). These
// hit the `Ok(token)` sigil branch rather than the `Err(())` branch.
const META: &str = "f j:t>R n t;r=jpar! j;r.MetaData";
const LEFT: &str = "f j:t>R n t;r=jpar! j;r.LeftValue";

fn check_meta(engine: &str) {
    assert_eq!(
        run(engine, META, "f", &[r#"{"MetaData":"hello"}"#]),
        "hello",
        "engine={engine}"
    );
}

fn check_left(engine: &str) {
    assert_eq!(
        run(engine, LEFT, "f", &[r#"{"LeftValue":99}"#]),
        "99",
        "engine={engine}"
    );
}

#[test]
fn leading_sigil_meta_tree() {
    check_meta("--run-tree");
}

#[test]
fn leading_sigil_meta_vm() {
    check_meta("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn leading_sigil_meta_cranelift() {
    check_meta("--run-cranelift");
}

#[test]
fn leading_sigil_left_tree() {
    check_left("--run-tree");
}

#[test]
fn leading_sigil_left_vm() {
    check_left("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn leading_sigil_left_cranelift() {
    check_left("--run-cranelift");
}

// ---- Negative regressions: strict lowercase rule preserved for bindings ----

#[test]
fn leading_upper_binding_still_errors_non_sigil() {
    // `URL=5` in a binding position must still emit ILO-L001 (logos rejects
    // the leading `U` as an unexpected character) — the post-dot fix must
    // not affect binding-position lexing.
    let err = run_err("f>n;URL=5;URL");
    assert!(!err.is_empty(), "expected an error, got empty stderr");
    // The fix only triggers post-dot; binding position should still reject.
    assert!(
        err.contains("ILO-L001") || err.contains("ILO-L003"),
        "expected lex error, got: {err}"
    );
}

#[test]
fn leading_upper_binding_still_errors_sigil() {
    // `MetaData=5` — `M` is a sigil but in binding position it shouldn't
    // be absorbed into an Ident.
    let err = run_err("f>n;MetaData=5;MetaData");
    assert!(!err.is_empty(), "expected an error, got empty stderr");
}
