/// ILO-391: World static capability enforcement.
///
/// Tests that the verifier emits ILO-T044 when a net builtin is called in a
/// scope that contains a World known statically to have net=false (via
/// `world-no-net`), and that valid patterns (no denied World in scope, dynamic
/// World, dynamic World from `world` builtin) are accepted.
use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn verify_src(src: &str) -> (String, bool) {
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

// ── world-no-net construction ─────────────────────────────────────────────────

#[test]
fn world_no_net_returns_world_with_net_false() {
    let (out, ok) = verify_src("main>W;world-no-net");
    assert!(ok, "world-no-net should succeed: {out}");
    assert!(out.contains("net: false"), "net should be false: {out}");
    assert!(out.contains("read: true"), "read should be true: {out}");
    assert!(out.contains("write: true"), "write should be true: {out}");
    assert!(out.contains("run: true"), "run should be true: {out}");
}

#[test]
fn world_no_net_field_net_is_false() {
    let (out, ok) = verify_src("main>b;wn=world-no-net;wn.net");
    assert!(ok, "wn.net should succeed: {out}");
    assert!(out.contains("false"), "wn.net should be false: {out}");
}

#[test]
fn world_no_net_type_is_world() {
    // world-no-net returns Ty::World — compatible with W param.
    let src = "use-w w:W>b;w.net\nmain>b;use-w world-no-net";
    let (out, ok) = verify_src(src);
    assert!(ok, "world-no-net should be accepted as W: {out}");
    assert!(out.contains("false"), "expected false: {out}");
}

// ── ILO-T044: static enforcement ─────────────────────────────────────────────

#[test]
fn get_with_world_no_net_in_scope_is_rejected() {
    // `get` called while a world-no-net binding is in scope → ILO-T044
    let src = "f url:t>R t t;wn=world-no-net;get url";
    let (out, ok) = verify_src(src);
    assert!(!ok, "should be rejected: {out}");
    assert!(out.contains("ILO-T044"), "expected ILO-T044: {out}");
    assert!(out.contains("wn"), "should mention the variable: {out}");
    assert!(out.contains("net=false"), "should mention net=false: {out}");
}

#[test]
fn pst_with_world_no_net_in_scope_is_rejected() {
    let src = "f url:t>R t t;wn=world-no-net;pst url \"body\"";
    let (out, ok) = verify_src(src);
    assert!(!ok, "should be rejected: {out}");
    assert!(out.contains("ILO-T044"), "expected ILO-T044: {out}");
}

#[test]
fn put_with_world_no_net_in_scope_is_rejected() {
    let src = "f url:t>R t t;wn=world-no-net;put url \"body\"";
    let (out, ok) = verify_src(src);
    assert!(!ok, "should be rejected: {out}");
    assert!(out.contains("ILO-T044"), "expected ILO-T044: {out}");
}

#[test]
fn del_with_world_no_net_in_scope_is_rejected() {
    let src = "f url:t>R t t;wn=world-no-net;del url";
    let (out, ok) = verify_src(src);
    assert!(!ok, "should be rejected: {out}");
    assert!(out.contains("ILO-T044"), "expected ILO-T044: {out}");
}

#[test]
fn hed_with_world_no_net_in_scope_is_rejected() {
    let src = "f url:t>R t t;wn=world-no-net;hed url";
    let (out, ok) = verify_src(src);
    assert!(!ok, "should be rejected: {out}");
    assert!(out.contains("ILO-T044"), "expected ILO-T044: {out}");
}

// ── valid patterns: no ILO-T044 ───────────────────────────────────────────────

#[test]
fn get_without_world_no_net_is_accepted() {
    // No net-denied World in scope → verifier accepts (runtime handles caps).
    let src = "f url:t>R t t;get url";
    let (out, ok) = verify_src(src);
    // May fail at runtime (no url arg) but should not be a verify error.
    assert!(
        ok || !out.contains("ILO-T044"),
        "should not emit ILO-T044: {out}"
    );
}

#[test]
fn get_with_dynamic_world_in_scope_is_not_rejected() {
    // `world` (dynamic) does not trigger ILO-T044 — net cap is runtime-determined.
    let src = "f url:t>R t t;w=world;get url";
    let (out, ok) = verify_src(src);
    assert!(
        ok || !out.contains("ILO-T044"),
        "dynamic world should not trigger ILO-T044: {out}"
    );
}

#[test]
fn world_no_net_without_net_call_is_accepted() {
    // Constructing a world-no-net but not calling a net builtin is fine.
    let src = "main>b;wn=world-no-net;wn.read";
    let (out, ok) = verify_src(src);
    assert!(ok, "no net call → should be accepted: {out}");
}

#[test]
fn world_param_with_net_call_is_not_rejected() {
    // A W parameter (dynamic, not statically denied) should not trigger ILO-T044.
    let src = "fetch w:W url:t>R t t;get url";
    let (out, ok) = verify_src(src);
    assert!(
        ok || !out.contains("ILO-T044"),
        "W param should not trigger ILO-T044: {out}"
    );
}
