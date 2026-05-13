// Regression tests for `at s i` on a text (string) value.
//
// Background: `at s i` on a string used to allocate a fresh `Vec<char>` on
// every call (`s.chars().collect()` in interpreter/VM/Cranelift). That made
// per-char loops like `@i 0..len s{c=at s i}` O(n²) in time and n in
// allocations, which manifested as apparent OOMs in NLP workloads at corpus
// scale (Moby Dick, 222k tokens).
//
// This file pins:
//   1. Correctness across ASCII and unicode strings, positive and negative
//      indices, on all three engines.
//   2. A scaling sanity check: a 50k-char per-char loop finishes well inside
//      a wall-clock budget that the old O(n²) implementation would blow.

use std::process::Command;
use std::time::{Duration, Instant};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// at on a text yields a single-character text.
const ASCII_FIRST_SRC: &str = "f>t;at \"hello\" 0";
const ASCII_LAST_SRC: &str = "f>t;at \"hello\" 4";
const ASCII_NEG_LAST_SRC: &str = "f>t;at \"hello\" -1";
const ASCII_NEG_FIRST_SRC: &str = "f>t;at \"hello\" -5";

fn check_eq(engine: &str, src: &str, expected: &str) {
    assert_eq!(run(engine, src, "f"), expected, "engine={engine} src={src}");
}

#[test]
fn at_text_ascii_tree() {
    check_eq("--run-tree", ASCII_FIRST_SRC, "h");
    check_eq("--run-tree", ASCII_LAST_SRC, "o");
    check_eq("--run-tree", ASCII_NEG_LAST_SRC, "o");
    check_eq("--run-tree", ASCII_NEG_FIRST_SRC, "h");
}

#[test]
fn at_text_ascii_vm() {
    check_eq("--run-vm", ASCII_FIRST_SRC, "h");
    check_eq("--run-vm", ASCII_LAST_SRC, "o");
    check_eq("--run-vm", ASCII_NEG_LAST_SRC, "o");
    check_eq("--run-vm", ASCII_NEG_FIRST_SRC, "h");
}

#[test]
#[cfg(feature = "cranelift")]
fn at_text_ascii_cranelift() {
    check_eq("--run-cranelift", ASCII_FIRST_SRC, "h");
    check_eq("--run-cranelift", ASCII_LAST_SRC, "o");
    check_eq("--run-cranelift", ASCII_NEG_LAST_SRC, "o");
    check_eq("--run-cranelift", ASCII_NEG_FIRST_SRC, "h");
}

// Unicode: "naïve" — 5 codepoints, 6 bytes. at returns codepoint-indexed chars,
// not byte-indexed slices.
const UNI_MID_SRC: &str = "f>t;at \"naïve\" 2";
const UNI_LAST_SRC: &str = "f>t;at \"naïve\" 4";
const UNI_NEG_MID_SRC: &str = "f>t;at \"naïve\" -3";
const UNI_NEG_LAST_SRC: &str = "f>t;at \"naïve\" -1";

#[test]
fn at_text_unicode_tree() {
    check_eq("--run-tree", UNI_MID_SRC, "ï");
    check_eq("--run-tree", UNI_LAST_SRC, "e");
    check_eq("--run-tree", UNI_NEG_MID_SRC, "ï");
    check_eq("--run-tree", UNI_NEG_LAST_SRC, "e");
}

#[test]
fn at_text_unicode_vm() {
    check_eq("--run-vm", UNI_MID_SRC, "ï");
    check_eq("--run-vm", UNI_LAST_SRC, "e");
    check_eq("--run-vm", UNI_NEG_MID_SRC, "ï");
    check_eq("--run-vm", UNI_NEG_LAST_SRC, "e");
}

#[test]
#[cfg(feature = "cranelift")]
fn at_text_unicode_cranelift() {
    check_eq("--run-cranelift", UNI_MID_SRC, "ï");
    check_eq("--run-cranelift", UNI_LAST_SRC, "e");
    check_eq("--run-cranelift", UNI_NEG_MID_SRC, "ï");
    check_eq("--run-cranelift", UNI_NEG_LAST_SRC, "e");
}

// Out-of-range on text: tree/vm error, cranelift returns nil (mirrors hd/list).
const TEXT_OOR_SRC: &str = "f>t;at \"abc\" 99";

#[test]
fn at_text_oor_tree() {
    let out = ilo()
        .args([TEXT_OOR_SRC, "--run-tree", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success(), "expected error");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("range") || stderr.contains("ILO-R009"),
        "stderr={stderr}"
    );
}

#[test]
fn at_text_oor_vm() {
    let out = ilo()
        .args([TEXT_OOR_SRC, "--run-vm", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success(), "expected error");
}

#[test]
#[cfg(feature = "cranelift")]
fn at_text_oor_cranelift() {
    let out = ilo()
        .args([TEXT_OOR_SRC, "--run-cranelift", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(out.status.success(), "expected nil (success)");
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("nil"),
        "stdout should be nil"
    );
}

// --- Scaling sanity check ---------------------------------------------------
//
// Build a 50_000-char string by doubling "A" then take every char with `at`
// in a loop. The old O(n²) impl took >5s on every engine; the new path runs
// in well under a second. Budget at 15s wall-clock leaves generous slack for
// slow CI runners but still catches a regression to quadratic behaviour
// (which would be tens of seconds on tree at 50k).

fn run_with_budget(engine: &str, src: &str, entry: &str, budget: Duration) -> String {
    let start = Instant::now();
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    let elapsed = start.elapsed();
    assert!(
        out.status.success(),
        "ilo {engine} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        elapsed < budget,
        "engine={engine} ran in {elapsed:?}, budget {budget:?} — \
         scaling regression in `at s i`? (likely a return to O(n²) chars().collect)",
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// Build a 50_000-char string by appending one char at a time, then call
// `at s i` for every index and tally a fingerprint so the work isn't optimised
// away. Build+at is the cost we measure; the budget is generous enough that
// only a return to O(n²) per-`at` allocation can trip it.
const AT_LOOP_SRC: &str = "f>n;\
    s=\"\";@k 0..50000{s=+s \"A\"};\
    l=len s;n=0;\
    @i 0..l{c=at s i;n=+n 1};\
    n";

#[test]
fn at_loop_scales_linearly_tree() {
    let out = run_with_budget("--run-tree", AT_LOOP_SRC, "f", Duration::from_secs(30));
    assert_eq!(out, "50000", "fingerprint mismatch");
}

#[test]
fn at_loop_scales_linearly_vm() {
    let out = run_with_budget("--run-vm", AT_LOOP_SRC, "f", Duration::from_secs(30));
    assert_eq!(out, "50000");
}

#[test]
#[cfg(feature = "cranelift")]
fn at_loop_scales_linearly_cranelift() {
    let out = run_with_budget("--run-cranelift", AT_LOOP_SRC, "f", Duration::from_secs(30));
    assert_eq!(out, "50000");
}
