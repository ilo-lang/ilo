// Regression tests for the `rgxall` builtin.
// rgxall pattern text -> L (L t): every match as a list of capture groups.
// No-group patterns wrap the whole match in a single-element inner list,
// so the outer shape stays predictable regardless of group count.
//
// Engine coverage: tree-only.
//
// The sibling builtin `rgx` is also tree-only today (both --run-vm and
// --run-cranelift report `Compile error: undefined function: rgx` on main).
// The VM compiler's "falls through to OP_CALL → interpreter" comment at
// src/vm/mod.rs:2394-2397 is aspirational, not current — there is no
// builtin bridge in OP_CALL's dispatcher today. Wiring rgx/rgxall through
// the VM and cranelift is a separate, larger fix (touches verify, VM
// compile, JIT compile) and out of scope for this PR. Keeping rgxall
// tree-only matches rgx's actual reality; when the VM bridge lands, both
// will switch on together and these tests should grow to check all three
// engines.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_text(src: &str) -> String {
    let out = ilo()
        .args([src, "--run-tree", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo --run-tree failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn check(src: &str, expected: &str) {
    let actual = run_text(src);
    assert_eq!(
        actual, expected,
        "src=`{src}`: got `{actual}`, expected `{expected}`"
    );
}

#[test]
fn rgxall_no_match_returns_empty_list() {
    check(r#"f>L (L t);rgxall "\d+" "no digits here""#, "[]");
}

#[test]
fn rgxall_single_match_no_groups() {
    check(r#"f>L (L t);rgxall "\d+" "abc 42 def""#, "[[42]]");
}

#[test]
fn rgxall_multiple_matches_no_groups() {
    // No-group case wraps each whole match in a single-element inner list,
    // preserving the uniform L (L t) shape.
    check(
        r#"f>L (L t);rgxall "\d+" "a1 b22 c333""#,
        "[[1], [22], [333]]",
    );
}

#[test]
fn rgxall_multiple_matches_one_group() {
    // The real-world HTML-scrape case: pull the inner text of every <h2>.
    // `rgx` silently returns only the first match here; `rgxall` returns
    // all of them.
    check(
        r#"f>L (L t);rgxall "<h2>([^<]+)</h2>" "<h2>One</h2> <h2>Two</h2> <h2>Three</h2>""#,
        "[[One], [Two], [Three]]",
    );
}

#[test]
fn rgxall_multiple_matches_multiple_groups() {
    // Two groups per match: every inner list has length 2.
    check(
        r#"f>L (L t);rgxall "(\w+)=(\d+)" "x=1 y=22 z=333""#,
        "[[x, 1], [y, 22], [z, 333]]",
    );
}

#[test]
fn rgxall_unicode_input() {
    check(
        r#"f>L (L t);rgxall "\w+" "café résumé naïve""#,
        "[[café], [résumé], [naïve]]",
    );
}

#[test]
fn rgxall_invalid_pattern_errors() {
    // Unclosed group is a regex compile error; must surface as a runtime error.
    let out = ilo()
        .args([r#"f>L (L t);rgxall "(unclosed" "input""#, "--run-tree", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected failure on invalid regex pattern"
    );
}
