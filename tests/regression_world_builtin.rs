/// ILO-68: World / capability parameter at the language level.
///
/// Tests that:
/// 1. `world` builtin works in permissive mode (all caps true).
/// 2. `w:W` is accepted as a function parameter type.
/// 3. World field access (`.net`, `.read`, `.write`, `.run`) returns Bool.
/// 4. A pure function with no `W` param verifies and runs cleanly.
/// 5. A function with `w:W` can be called with the world token.
use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_src(src: &str) -> (String, bool) {
    let out = ilo().arg(src).output().expect("ilo binary not found");
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    let combined = if stderr.is_empty() {
        stdout
    } else {
        format!("{stdout}\n{stderr}")
    };
    (combined, out.status.success())
}

#[test]
fn world_builtin_returns_world_type() {
    // `world` in permissive mode - output should show all true
    let (out, ok) = run_src("main>W;world");
    assert!(ok, "world should succeed: {out}");
    assert!(out.contains("net: true"), "net should be true: {out}");
    assert!(out.contains("read: true"), "read should be true: {out}");
    assert!(out.contains("write: true"), "write should be true: {out}");
    assert!(out.contains("run: true"), "run should be true: {out}");
}

#[test]
fn world_field_net() {
    let (out, ok) = run_src("main>b;w=world;w.net");
    assert!(ok, "w.net should succeed: {out}");
    assert_eq!(
        out.trim_end_matches(|c: char| !c.is_alphanumeric()).trim(),
        "true",
        "net={out}"
    );
}

#[test]
fn world_field_read() {
    let (out, ok) = run_src("main>b;w=world;w.read");
    assert!(ok, "w.read should succeed: {out}");
    assert!(out.contains("true"), "read should be true: {out}");
}

#[test]
fn world_field_write() {
    let (out, ok) = run_src("main>b;w=world;w.write");
    assert!(ok, "w.write should succeed: {out}");
    assert!(out.contains("true"), "write should be true: {out}");
}

#[test]
fn world_field_run() {
    let (out, ok) = run_src("main>b;w=world;w.run");
    assert!(ok, "w.run should succeed: {out}");
    assert!(out.contains("true"), "run should be true: {out}");
}

#[test]
fn fn_with_world_param_works() {
    // A function that accepts w:W and accesses w.net
    let src = "do-io w:W>b;w.net\nmain>b;do-io world";
    let (out, ok) = run_src(src);
    assert!(ok, "fn with w:W should work: {out}");
    assert!(out.contains("true"), "expected true: {out}");
}

#[test]
fn pure_fn_no_world_works() {
    // A pure function with no W param works unchanged
    let src = "add x:n y:n>n;+x y\nmain>n;add 3 4";
    let (out, ok) = run_src(src);
    assert!(ok, "pure fn should work: {out}");
    assert!(out.contains('7'), "expected 7: {out}");
}

#[test]
fn world_w_type_token_in_sig() {
    // `W` in function return type parses without error
    let src = "get-w>W;world\nmain>b;w=get-w;w.net";
    let (out, ok) = run_src(src);
    assert!(ok, "W return type should work: {out}");
    assert!(out.contains("true"), "expected true: {out}");
}

#[test]
fn world_type_not_undefined_type_error() {
    // Verifier should NOT emit ILO-T003 for W type
    let src = "use-w w:W>W;w\nmain>W;use-w world";
    let (out, ok) = run_src(src);
    // If ILO-T003 fires it would fail
    assert!(ok, "W type should not cause ILO-T003: {out}");
}
