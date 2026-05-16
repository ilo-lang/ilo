// Tree-engine regression tests for the `flat` builtin: flatten a list one
// level. Signature: `flat xs:L (L a) > L a`. Non-list elements pass through
// unchanged.
//
// Cross-engine coverage (tree + VM + Cranelift) lives in
// `tests/regression_flat_cross_engine.rs`. The two `*_undefined` tests
// that used to live here were removed when `flat` was wired through VM
// and Cranelift in `fix/flat-cross-engine`.

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
