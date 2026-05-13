// Regression tests for the `rgxall` builtin.
// rgxall pattern text -> L (L t): every match as a list of capture groups.
// No-group patterns wrap the whole match in a single-element inner list,
// so the outer shape stays predictable regardless of group count.
//
// Engine coverage: tree, VM, Cranelift JIT. The tree-only restriction
// noted in the original landing PR is gone — `rgxall` (and its siblings
// `rgx`, `fmt` variadic, 2-arg `rd`, `rdb`) now route through the generic
// `OP_CALL_BUILTIN_TREE` bridge in the VM and Cranelift JIT, so every
// engine produces identical output.

use std::process::Command;

const ENGINES: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_text_engine(src: &str, engine: &str) -> String {
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

fn check(src: &str, expected: &str) {
    for engine in ENGINES {
        let actual = run_text_engine(src, engine);
        assert_eq!(
            actual, expected,
            "engine={engine}, src=`{src}`: got `{actual}`, expected `{expected}`"
        );
    }
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
fn rgxall_alternation_absent_groups_filtered() {
    // `(a)|(b)` against "a b": the matching branch contributes its group,
    // the absent branch is filtered out (via captures.get(i).map). Inner
    // list length tracks *participating* groups, not declared groups. This
    // matches rgx's existing semantics and is the documented behaviour.
    check(r#"f>L (L t);rgxall "(a)|(b)" "a b""#, "[[a], [b]]");
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
