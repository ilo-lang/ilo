// Regression tests for the `rgxall1` builtin.
//
// rgxall1 pattern text -> L t: first-capture-group convenience over rgxall.
// - 0 groups: flat list of whole matches (parallel to rgx no-group, but
//   multi-match — rgx no-group returns all matches, rgxall1 no-group does
//   the same shape).
// - 1 group: flat list of capture-1 strings.
// - 2+ groups: runtime error pointing at rgxall.
//
// Motivation: the html-scraper persona log (rerun5, line 4988 in
// ilo_assessment_feedback.md) flagged the `ext xs:L t>t;hd xs` flatten
// helper as the last line of rent across 5 reruns. `titles=rgxall1
// "<a class=\"titleline\"[^>]*>([^<]+)" html` removes the helper entirely.
//
// Engine coverage: tree, VM, Cranelift JIT, all via the tree-bridge.

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
fn rgxall1_no_match_returns_empty_list() {
    check(r#"f>L t;rgxall1 "\d+" "no digits here""#, "[]");
}

#[test]
fn rgxall1_no_groups_returns_whole_matches() {
    // Parallel to rgx no-group, but returns every match instead of just
    // the first.
    check(r#"f>L t;rgxall1 "\d+" "a1 b22 c333""#, "[1, 22, 333]");
}

#[test]
fn rgxall1_one_group_returns_flat_captures() {
    // The headline use case: pull inner text of every <h2> as a flat list.
    // With rgxall this returns [[One], [Two], [Three]] and needs a flatten
    // helper. rgxall1 returns the flat list directly.
    check(
        r#"f>L t;rgxall1 "<h2>([^<]+)</h2>" "<h2>One</h2> <h2>Two</h2> <h2>Three</h2>""#,
        "[One, Two, Three]",
    );
}

#[test]
fn rgxall1_html_scraper_persona_shape() {
    // The exact shape from html-scraper rerun5: extract the inner text of
    // every titleline anchor. Removes the `ext xs:L t>t;hd xs` helper.
    check(
        r#"f>L t;rgxall1 "<a class=\"titleline\">([^<]+)</a>" "<a class=\"titleline\">Hello</a> junk <a class=\"titleline\">World</a>""#,
        "[Hello, World]",
    );
}

#[test]
fn rgxall1_alternation_absent_groups_filtered() {
    // `(a)|(b)` declares 2 groups → rejected as multi-group. The single-
    // group form below tests participation filtering: `(a)+` declares 1
    // group, matches all `a` runs.
    check(r#"f>L t;rgxall1 "(a+)" "aa b aaa""#, "[aa, aaa]");
}

#[test]
fn rgxall1_unicode_captures() {
    check(
        r#"f>L t;rgxall1 "(\w+)" "café résumé naïve""#,
        "[café, résumé, naïve]",
    );
}

#[test]
fn rgxall1_multiple_groups_errors_with_hint_to_rgxall() {
    // 2-group pattern must error at runtime with a message pointing the
    // user at rgxall, which preserves every group on every match.
    for engine in ENGINES {
        let out = ilo()
            .args([r#"f>L t;rgxall1 "(\w+)=(\d+)" "x=1 y=22""#, engine, "f"])
            .output()
            .expect("failed to run ilo");
        assert!(
            !out.status.success(),
            "engine={engine}: expected failure on multi-group rgxall1"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("rgxall1") && stderr.contains("rgxall"),
            "engine={engine}: stderr should mention rgxall1 + rgxall, got `{stderr}`"
        );
    }
}

#[test]
fn rgxall1_invalid_pattern_errors() {
    let out = ilo()
        .args([r#"f>L t;rgxall1 "(unclosed" "input""#, "--run-tree", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected failure on invalid regex pattern"
    );
}
