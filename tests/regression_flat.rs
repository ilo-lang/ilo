// Regression tests for the `flat` builtin: flatten a list one level.
// Signature: `flat xs:L (L a) > L a`. Non-list elements pass through
// unchanged. `flat` is currently tree-engine only; vm/cranelift report
// an "undefined function" compile error.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_ok(engine: &str, src: &str) -> String {
    let out = ilo()
        .args([src, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(engine: &str, src: &str) -> String {
    let out = ilo()
        .args([src, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} unexpectedly succeeded for `{src}`: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).to_string() + &String::from_utf8_lossy(&out.stdout)
}

#[test]
fn flat_basic_nested_tree() {
    assert_eq!(
        run_ok("--run-tree", "f>L n;flat [[1, 2], [3, 4]]"),
        "[1, 2, 3, 4]"
    );
}

#[test]
fn flat_empty_outer_tree() {
    assert_eq!(run_ok("--run-tree", "f>L n;flat []"), "[]");
}

#[test]
fn flat_inner_empties_dropped_tree() {
    // [[1], [], [2]] → [1, 2]
    assert_eq!(run_ok("--run-tree", "f>L n;flat [[1], [], [2]]"), "[1, 2]");
}

#[test]
fn flat_single_level_passes_through_tree() {
    // Non-list elements pass through unchanged: flat is "flatten one level",
    // a list of scalars is returned with the scalars in place.
    assert_eq!(run_ok("--run-tree", "f>L n;flat [1, 2, 3]"), "[1, 2, 3]");
}

#[test]
fn flat_mixed_passes_non_list_through_tree() {
    // Mixed list: nested lists are spliced, scalars are kept in place.
    assert_eq!(
        run_ok("--run-tree", "f>L n;flat [[1, 2], 3, [4, 5]]"),
        "[1, 2, 3, 4, 5]"
    );
}

// vm/cranelift do not implement `flat` yet — they should reject the
// program at compile time with an "undefined function" error rather
// than producing a wrong answer silently.

#[test]
fn flat_vm_undefined() {
    let err = run_err("--run-vm", "f>L n;flat [[1, 2], [3, 4]]");
    assert!(
        err.contains("flat") || err.contains("undefined"),
        "expected vm to report missing flat, got: {err}"
    );
}

#[cfg(feature = "cranelift")]
#[test]
fn flat_cranelift_undefined() {
    let err = run_err("--run-cranelift", "f>L n;flat [[1, 2], [3, 4]]");
    assert!(
        err.contains("flat") || err.contains("undefined"),
        "expected cranelift to report missing flat, got: {err}"
    );
}
