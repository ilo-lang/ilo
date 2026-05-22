/// ILO-393: World multi-world composition — world-and / world-or.
///
/// Tests that `world-and` (intersection) and `world-or` (union) produce
/// correctly combined World values at runtime.
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

// ── world-and (intersection) ──────────────────────────────────────────────────

#[test]
fn world_and_true_true_net() {
    // world AND world = world (all caps true under permissive Caps)
    let (out, ok) = run_src("main>b;(world-and world world).net");
    assert!(ok, "world-and world world should succeed: {out}");
    assert!(out.contains("true"), "net should be true: {out}");
}

#[test]
fn world_and_false_true_net() {
    // (no-net world) AND world → net=false
    let (out, ok) = run_src("main>b;(world-and (no-net world) world).net");
    assert!(ok, "world-and should succeed: {out}");
    assert!(out.contains("false"), "net should be false: {out}");
}

#[test]
fn world_and_true_false_net() {
    // world AND (no-net world) → net=false
    let (out, ok) = run_src("main>b;(world-and world (no-net world)).net");
    assert!(ok, "world-and should succeed: {out}");
    assert!(out.contains("false"), "net should be false: {out}");
}

#[test]
fn world_and_false_false_net() {
    // (no-net) AND (no-net) → net=false
    let (out, ok) = run_src("main>b;(world-and (no-net world) (no-net world)).net");
    assert!(ok, "world-and should succeed: {out}");
    assert!(out.contains("false"), "net should be false: {out}");
}

#[test]
fn world_and_read_only_no_net_read() {
    // (read-only w) AND (no-net w) → read=true (both keep it)
    let (out, ok) =
        run_src("main>b;(world-and (read-only world) (no-net world)).read");
    assert!(ok, "world-and should succeed: {out}");
    assert!(out.contains("true"), "read should be true: {out}");
}

#[test]
fn world_and_read_only_no_net_write() {
    // (read-only w) AND (no-net w) → write=false (read-only denies it)
    let (out, ok) =
        run_src("main>b;(world-and (read-only world) (no-net world)).write");
    assert!(ok, "world-and should succeed: {out}");
    assert!(out.contains("false"), "write should be false: {out}");
}

#[test]
fn world_and_read_only_no_net_run() {
    // (read-only w) AND (no-net w) → run=false (read-only denies it)
    let (out, ok) =
        run_src("main>b;(world-and (read-only world) (no-net world)).run");
    assert!(ok, "world-and should succeed: {out}");
    assert!(out.contains("false"), "run should be false: {out}");
}

#[test]
fn world_and_idempotent() {
    // world AND world = world for all caps (true under permissive)
    let (out, ok) = run_src(
        "main>b;w=world;wa=world-and w w;prnt wa.net;prnt wa.write;wa.run",
    );
    assert!(ok, "world-and idempotent should succeed: {out}");
    for line in out.lines().filter(|l| !l.starts_with('{')) {
        assert!(line.contains("true"), "all caps should be true: {line}");
    }
}

#[test]
fn world_and_type_is_world() {
    // The result of world-and is a World value (has .net field)
    let (out, ok) = run_src(
        "use-w w:W>b;w.read\nmain>b;use-w (world-and world world)",
    );
    assert!(ok, "world-and result should be World: {out}");
    assert!(out.contains("true"), "read should be true: {out}");
}

// ── world-or (union) ──────────────────────────────────────────────────────────

#[test]
fn world_or_false_false_net() {
    // (no-net w) OR (no-net w) → net=false
    let (out, ok) = run_src("main>b;(world-or (no-net world) (no-net world)).net");
    assert!(ok, "world-or should succeed: {out}");
    assert!(out.contains("false"), "net should be false: {out}");
}

#[test]
fn world_or_true_false_net() {
    // world OR (no-net w) → net=true
    let (out, ok) = run_src("main>b;(world-or world (no-net world)).net");
    assert!(ok, "world-or should succeed: {out}");
    assert!(out.contains("true"), "net should be true: {out}");
}

#[test]
fn world_or_false_true_net() {
    // (no-net w) OR world → net=true
    let (out, ok) = run_src("main>b;(world-or (no-net world) world).net");
    assert!(ok, "world-or should succeed: {out}");
    assert!(out.contains("true"), "net should be true: {out}");
}

#[test]
fn world_or_net_only_read_only_net() {
    // (net-only w) OR (read-only w) → net=true (net-only keeps it)
    let (out, ok) =
        run_src("main>b;(world-or (net-only world) (read-only world)).net");
    assert!(ok, "world-or should succeed: {out}");
    assert!(out.contains("true"), "net should be true: {out}");
}

#[test]
fn world_or_net_only_read_only_read() {
    // (net-only w) OR (read-only w) → read=true (read-only keeps it)
    let (out, ok) =
        run_src("main>b;(world-or (net-only world) (read-only world)).read");
    assert!(ok, "world-or should succeed: {out}");
    assert!(out.contains("true"), "read should be true: {out}");
}

#[test]
fn world_or_net_only_read_only_write() {
    // (net-only w) OR (read-only w) → write=false (both deny it)
    let (out, ok) =
        run_src("main>b;(world-or (net-only world) (read-only world)).write");
    assert!(ok, "world-or should succeed: {out}");
    assert!(out.contains("false"), "write should be false: {out}");
}

#[test]
fn world_or_idempotent() {
    // world OR world = world for all caps
    let (out, ok) = run_src(
        "main>b;w=world;wo=world-or w w;prnt wo.net;prnt wo.write;wo.run",
    );
    assert!(ok, "world-or idempotent should succeed: {out}");
    for line in out.lines().filter(|l| !l.starts_with('{')) {
        assert!(line.contains("true"), "all caps should be true: {line}");
    }
}

#[test]
fn world_or_type_is_world() {
    // The result of world-or is a World value (has .net field)
    let (out, ok) = run_src(
        "use-w w:W>b;w.write\nmain>b;use-w (world-or world world)",
    );
    assert!(ok, "world-or result should be World: {out}");
    assert!(out.contains("true"), "write should be true: {out}");
}

// ── chained composition ───────────────────────────────────────────────────────

#[test]
fn world_and_then_or_net() {
    // ((no-net w) AND (no-net w)) OR world → net=true (OR restores it)
    let (out, ok) = run_src(
        "main>b;a=world-and (no-net world) (no-net world);(world-or a world).net",
    );
    assert!(ok, "chained and/or should succeed: {out}");
    assert!(out.contains("true"), "net should be true after OR: {out}");
}

#[test]
fn world_or_then_and_no_net() {
    // (world OR world) AND (no-net w) → net=false (AND restricts)
    let (out, ok) = run_src(
        "main>b;o=world-or world world;(world-and o (no-net world)).net",
    );
    assert!(ok, "chained or/and should succeed: {out}");
    assert!(out.contains("false"), "net should be false after AND: {out}");
}

// ── error cases ───────────────────────────────────────────────────────────────

#[test]
fn world_and_non_world_first_arg_errors() {
    let (out, ok) = run_src("main>b;world-and 1 world");
    assert!(!ok, "world-and with non-World first arg should fail: {out}");
    assert!(
        out.contains("ILO-R009") || out.contains("World"),
        "should mention error: {out}"
    );
}

#[test]
fn world_and_non_world_second_arg_errors() {
    let (out, ok) = run_src("main>b;world-and world 1");
    assert!(!ok, "world-and with non-World second arg should fail: {out}");
    assert!(
        out.contains("ILO-R009") || out.contains("World"),
        "should mention error: {out}"
    );
}

#[test]
fn world_or_non_world_first_arg_errors() {
    let (out, ok) = run_src("main>b;world-or \"x\" world");
    assert!(!ok, "world-or with non-World first arg should fail: {out}");
    assert!(
        out.contains("ILO-R009") || out.contains("World"),
        "should mention error: {out}"
    );
}

#[test]
fn world_or_non_world_second_arg_errors() {
    let (out, ok) = run_src("main>b;world-or world \"x\"");
    assert!(!ok, "world-or with non-World second arg should fail: {out}");
    assert!(
        out.contains("ILO-R009") || out.contains("World"),
        "should mention error: {out}"
    );
}
