/// ILO-392: World sub-world masking — read-only, net-only, no-net.
///
/// Tests that the three sub-world masking builtins produce correctly masked
/// World values at runtime, and that the static enforcer (ILO-T044) rejects
/// net/write/run builtins called in a scope containing a statically-denied World.
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
    } else if stdout.is_empty() {
        stderr
    } else {
        format!("{stdout}\n{stderr}")
    };
    (combined, out.status.success())
}

// ── read-only ──────────────────────────────────────────────────────────────────

#[test]
fn read_only_net_is_false() {
    let (out, ok) = run_src("main>b;ro=read-only world;ro.net");
    assert!(ok, "read-only world should succeed: {out}");
    assert!(out.contains("false"), "net should be false: {out}");
}

#[test]
fn read_only_write_is_false() {
    let (out, ok) = run_src("main>b;ro=read-only world;ro.write");
    assert!(ok, "read-only world should succeed: {out}");
    assert!(out.contains("false"), "write should be false: {out}");
}

#[test]
fn read_only_run_is_false() {
    let (out, ok) = run_src("main>b;ro=read-only world;ro.run");
    assert!(ok, "read-only world should succeed: {out}");
    assert!(out.contains("false"), "run should be false: {out}");
}

#[test]
fn read_only_read_is_true() {
    // Under the default permissive Caps, read=true → preserved by read-only.
    let (out, ok) = run_src("main>b;ro=read-only world;ro.read");
    assert!(ok, "read-only world should succeed: {out}");
    assert!(out.contains("true"), "read should be true: {out}");
}

#[test]
fn read_only_type_is_world() {
    // Parentheses required in argument position (call precedence).
    let src = "use-w w:W>b;w.read\nmain>b;use-w (read-only world)";
    let (out, ok) = run_src(src);
    assert!(ok, "read-only should be accepted as W: {out}");
}

// ── net-only ───────────────────────────────────────────────────────────────────

#[test]
fn net_only_read_is_false() {
    let (out, ok) = run_src("main>b;no=net-only world;no.read");
    assert!(ok, "net-only world should succeed: {out}");
    assert!(out.contains("false"), "read should be false: {out}");
}

#[test]
fn net_only_write_is_false() {
    let (out, ok) = run_src("main>b;no=net-only world;no.write");
    assert!(ok, "net-only world should succeed: {out}");
    assert!(out.contains("false"), "write should be false: {out}");
}

#[test]
fn net_only_run_is_false() {
    let (out, ok) = run_src("main>b;no=net-only world;no.run");
    assert!(ok, "net-only world should succeed: {out}");
    assert!(out.contains("false"), "run should be false: {out}");
}

#[test]
fn net_only_net_is_true() {
    // Under permissive Caps, net=true → preserved by net-only.
    let (out, ok) = run_src("main>b;no=net-only world;no.net");
    assert!(ok, "net-only world should succeed: {out}");
    assert!(out.contains("true"), "net should be true: {out}");
}

// ── no-net ─────────────────────────────────────────────────────────────────────

#[test]
fn no_net_net_is_false() {
    let (out, ok) = run_src("main>b;nn=no-net world;nn.net");
    assert!(ok, "no-net world should succeed: {out}");
    assert!(out.contains("false"), "net should be false: {out}");
}

#[test]
fn no_net_read_is_true() {
    let (out, ok) = run_src("main>b;nn=no-net world;nn.read");
    assert!(ok, "no-net world should succeed: {out}");
    assert!(out.contains("true"), "read should be true: {out}");
}

#[test]
fn no_net_write_is_true() {
    let (out, ok) = run_src("main>b;nn=no-net world;nn.write");
    assert!(ok, "no-net world should succeed: {out}");
    assert!(out.contains("true"), "write should be true: {out}");
}

#[test]
fn no_net_run_is_true() {
    let (out, ok) = run_src("main>b;nn=no-net world;nn.run");
    assert!(ok, "no-net world should succeed: {out}");
    assert!(out.contains("true"), "run should be true: {out}");
}

#[test]
fn no_net_type_is_world() {
    // Parentheses required in argument position (call precedence).
    let src = "use-w w:W>b;w.net\nmain>b;use-w (no-net world)";
    let (out, ok) = run_src(src);
    assert!(ok, "no-net should be accepted as W: {out}");
}

// ── ILO-T044 static enforcement: read-only denies net ─────────────────────────

#[test]
fn get_with_read_only_in_scope_is_rejected() {
    let src = "f url:t>R t t;ro=read-only world;get url";
    let (out, ok) = run_src(src);
    assert!(!ok, "should be rejected: {out}");
    assert!(out.contains("ILO-T044"), "expected ILO-T044: {out}");
    assert!(out.contains("ro"), "should mention the variable: {out}");
}

#[test]
fn pst_with_read_only_in_scope_is_rejected() {
    let src = "f url:t>R t t;ro=read-only world;pst url \"body\"";
    let (out, ok) = run_src(src);
    assert!(!ok, "should be rejected: {out}");
    assert!(out.contains("ILO-T044"), "expected ILO-T044: {out}");
}

// ── ILO-T044 static enforcement: no-net denies net ────────────────────────────

#[test]
fn get_with_no_net_in_scope_is_rejected() {
    let src = "f url:t>R t t;nn=no-net world;get url";
    let (out, ok) = run_src(src);
    assert!(!ok, "should be rejected: {out}");
    assert!(out.contains("ILO-T044"), "expected ILO-T044: {out}");
}

// ── ILO-T044 static enforcement: read-only / net-only deny write ───────────────

#[test]
fn wr_with_read_only_in_scope_is_rejected() {
    let src = "f path:t>_;ro=read-only world;wr path \"data\"";
    let (out, ok) = run_src(src);
    assert!(!ok, "should be rejected: {out}");
    assert!(out.contains("ILO-T044"), "expected ILO-T044: {out}");
}

#[test]
fn wr_with_net_only_in_scope_is_rejected() {
    let src = "f path:t>_;no=net-only world;wr path \"data\"";
    let (out, ok) = run_src(src);
    assert!(!ok, "should be rejected: {out}");
    assert!(out.contains("ILO-T044"), "expected ILO-T044: {out}");
}

// ── ILO-T044 static enforcement: read-only / net-only deny run ────────────────

#[test]
fn run_builtin_with_read_only_in_scope_is_rejected() {
    let src = "f cmd:t>_;ro=read-only world;run cmd []";
    let (out, ok) = run_src(src);
    assert!(!ok, "should be rejected: {out}");
    assert!(out.contains("ILO-T044"), "expected ILO-T044: {out}");
}

#[test]
fn run_builtin_with_net_only_in_scope_is_rejected() {
    let src = "f cmd:t>_;no=net-only world;run cmd []";
    let (out, ok) = run_src(src);
    assert!(!ok, "should be rejected: {out}");
    assert!(out.contains("ILO-T044"), "expected ILO-T044: {out}");
}

// ── valid: no-net does not deny write/run ─────────────────────────────────────

#[test]
fn wr_with_no_net_in_scope_is_accepted() {
    // no-net only masks net; write is still allowed → no ILO-T044
    let src = "f path:t>_;nn=no-net world;wr path \"data\"";
    let (out, ok) = run_src(src);
    assert!(
        ok || !out.contains("ILO-T044"),
        "no-net should not block write: {out}"
    );
}

#[test]
fn read_only_without_net_or_write_call_is_accepted() {
    let src = "main>b;ro=read-only world;ro.read";
    let (out, ok) = run_src(src);
    assert!(ok, "no net/write/run call → should be accepted: {out}");
}
