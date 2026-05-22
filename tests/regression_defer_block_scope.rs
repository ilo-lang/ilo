//! CLI-level tests for ILO-365: block-scope `defer` / `errdefer`.
//!
//! These tests spawn the `ilo` binary and capture stdout so they can verify
//! that `defer` fires the correct number of times (once per iteration) and
//! only in the taken branch — behaviour that is impossible to observe through
//! the `run_tree` return-value API because `defer` only accepts expressions,
//! not assignment statements.
//!
//! Each test runs with `--vm` (the production engine).

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn write_src(src: &str, tag: &str) -> std::path::PathBuf {
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "ilo_defer_block_{}_{}_{}.ilo",
        std::process::id(),
        seq,
        tag,
    ));
    std::fs::write(&path, src).unwrap();
    path
}

fn run_vm(src: &str, fn_name: &str) -> String {
    let path = write_src(src, fn_name);
    let out = ilo()
        .arg(path.to_str().unwrap())
        .arg("--vm")
        .arg(fn_name)
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo --vm failed for fn={fn_name} src=`{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

// ── foreach: defer fires once per iteration ──────────────────────────────────

/// `defer prnt i` in a foreach loop body must print once per element.
/// Three elements → three lines of output (each line is the element value),
/// plus the function return value auto-printed on the final line.
#[test]
fn defer_in_foreach_fires_per_iteration() {
    // Function iterates over [10 20 30]; each iteration defers `prnt i`.
    // The defer fires at the end of each iteration (block exit), so output is:
    //   10
    //   20
    //   30
    //   0        ← function return value (auto-print, last stmt is 0)
    let src = "f>n;@i [10 20 30]{defer prnt i};0";
    let out = run_vm(src, "f");
    let lines: Vec<&str> = out.trim().lines().collect();
    // The three defer-prints must appear, in order (LIFO within a single-item
    // stack is still the one item).
    assert!(
        lines.contains(&"10") && lines.contains(&"20") && lines.contains(&"30"),
        "expected 10, 20, 30 in output, got: {:?}",
        lines
    );
    // Count: the defer must fire exactly 3 times (once per iteration), so
    // there should be exactly 3 occurrences of the element values in output.
    let defer_lines: Vec<&&str> = lines
        .iter()
        .filter(|&&l| l == "10" || l == "20" || l == "30")
        .collect();
    assert_eq!(
        defer_lines.len(),
        3,
        "defer must fire exactly once per iteration (3 times), got {} firings in: {:?}",
        defer_lines.len(),
        lines
    );
}

/// `defer prnt i` in a forrange loop body must fire once per range step.
#[test]
fn defer_in_forrange_fires_per_iteration() {
    // Range 0..3 → iterations i=0, i=1, i=2.  Each iteration defers prnt i.
    // Expected defer output lines: 0, 1, 2 (one per iteration).
    let src = "f>n;@i 0..3{defer prnt i};99";
    let out = run_vm(src, "f");
    let lines: Vec<&str> = out.trim().lines().collect();
    // The three deferred prints must appear.
    assert!(
        lines.contains(&"0") && lines.contains(&"1") && lines.contains(&"2"),
        "expected 0, 1, 2 in output, got: {:?}",
        lines
    );
    let defer_count = lines
        .iter()
        .filter(|&&l| l == "0" || l == "1" || l == "2")
        .count();
    assert_eq!(
        defer_count, 3,
        "defer must fire exactly 3 times, got {} in: {:?}",
        defer_count, lines
    );
}

// ── if: defer fires only when branch taken ───────────────────────────────────

/// `defer prnt` inside the taken branch fires; inside the untaken branch it
/// does not.
#[test]
fn defer_in_if_fires_only_when_branch_taken() {
    // Ternary `=x 1{then}{else}`:
    //   taken (x=1):   defer prnt "taken"  → stdout has "taken"
    //   untaken (x=0): else branch has defer prnt "other"
    let src = "f x:n>n;=x 1{defer prnt \"taken\"}{defer prnt \"other\"};x";
    let path = write_src(src, "if_taken");

    // When x=1, the if branch fires its defer
    let out1 = ilo()
        .arg(path.to_str().unwrap())
        .arg("--vm")
        .arg("f")
        .arg("1")
        .output()
        .expect("failed to run");
    assert!(out1.status.success());
    let taken_out = String::from_utf8_lossy(&out1.stdout).to_string();
    assert!(
        taken_out.contains("taken"),
        "defer in taken branch must fire, got: {:?}",
        taken_out
    );
    assert!(
        !taken_out.contains("other"),
        "defer in untaken branch must NOT fire, got: {:?}",
        taken_out
    );

    // When x=0, the else branch fires its defer
    let out0 = ilo()
        .arg(path.to_str().unwrap())
        .arg("--vm")
        .arg("f")
        .arg("0")
        .output()
        .expect("failed to run");
    assert!(out0.status.success());
    let other_out = String::from_utf8_lossy(&out0.stdout).to_string();
    assert!(
        other_out.contains("other"),
        "defer in else branch must fire when x=0, got: {:?}",
        other_out
    );
    assert!(
        !other_out.contains("taken"),
        "defer in if branch must NOT fire when x=0, got: {:?}",
        other_out
    );
}

// ── match: defer fires only for matched arm ───────────────────────────────────

/// `defer prnt` in a match arm fires only for the arm whose pattern matches.
#[test]
fn defer_in_match_arm_fires_only_for_matched_arm() {
    // ? x { 1: {defer prnt "arm1"; x}; _: {defer prnt "arm2"; x} }
    let src = "f x:n>n;? x {1:{defer prnt \"arm1\";x};_:{defer prnt \"arm2\";x}}";

    let path = write_src(src, "match_arm");

    // x=1 → arm1 matches → "arm1" printed, "arm2" not printed
    let out1 = ilo()
        .arg(path.to_str().unwrap())
        .arg("--vm")
        .arg("f")
        .arg("1")
        .output()
        .expect("failed to run");
    assert!(out1.status.success());
    let s1 = String::from_utf8_lossy(&out1.stdout).to_string();
    assert!(
        s1.contains("arm1"),
        "arm1's defer must fire for x=1, got: {:?}",
        s1
    );
    assert!(
        !s1.contains("arm2"),
        "arm2's defer must NOT fire for x=1, got: {:?}",
        s1
    );

    // x=2 → wildcard arm matches → "arm2" printed, "arm1" not printed
    let out2 = ilo()
        .arg(path.to_str().unwrap())
        .arg("--vm")
        .arg("f")
        .arg("2")
        .output()
        .expect("failed to run");
    assert!(out2.status.success());
    let s2 = String::from_utf8_lossy(&out2.stdout).to_string();
    assert!(
        s2.contains("arm2"),
        "arm2's defer must fire for x=2, got: {:?}",
        s2
    );
    assert!(
        !s2.contains("arm1"),
        "arm1's defer must NOT fire for x=2, got: {:?}",
        s2
    );
}

// ── while: defer fires per iteration ─────────────────────────────────────────

/// `defer prnt i` in a while loop body fires once per iteration.
#[test]
fn defer_in_while_fires_per_iteration() {
    // i starts at 0; while i < 3: i = i+1; defer prnt i
    // defer fires at block exit with i already incremented: 1, 2, 3 → 3 lines
    let src = "f>n;i=0;wh <i 3{i=+i 1;defer prnt i};99";
    let out = run_vm(src, "f");
    let lines: Vec<&str> = out.trim().lines().collect();
    let defer_count = lines
        .iter()
        .filter(|&&l| l == "1" || l == "2" || l == "3")
        .count();
    assert_eq!(
        defer_count, 3,
        "defer in while must fire 3 times, got {} in: {:?}",
        defer_count, lines
    );
}
