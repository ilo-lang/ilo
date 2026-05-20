// Regression tests for the `rgxall-multi` builtin.
//
// rgxall-multi pats:L t line:t -> L t
//
// Apply multiple patterns to a single line and collect all hits in
// pattern order into a flat list. For each pattern the semantics follow
// rgxall1: 0 capture groups -> whole matches; 1 capture group -> capture-1
// strings; 2+ capture groups -> ILO-R009.
//
// Motivation: cron-explainer and historical-archeologist personas both
// needed multi-pattern flat-match but had to spell it as the verbose
//   flat (map (p:t>L t;rgxall1 p line) pats)
// This builtin reduces that to one call.
//
// Engine coverage: tree, VM, Cranelift JIT — all via the tree-bridge.

use std::process::Command;

const ENGINES: &[&str] = &["--vm", "--jit"];

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(src: &str, engine: &str) -> String {
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
        let actual = run(src, engine);
        assert_eq!(
            actual, expected,
            "engine={engine}, src=`{src}`: got `{actual}`, expected `{expected}`"
        );
    }
}

fn check_error(src: &str, fragment: &str) {
    for engine in ENGINES {
        let out = ilo()
            .args([src, engine, "f"])
            .output()
            .expect("failed to run ilo");
        assert!(
            !out.status.success(),
            "engine={engine}: expected failure for `{src}`"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(fragment),
            "engine={engine}: stderr should contain `{fragment}`, got `{stderr}`"
        );
    }
}

// ---- basic correctness ----

#[test]
fn rgxall_multi_empty_pats_returns_empty() {
    check(r#"f>L t;rgxall-multi [] "some text here""#, "[]");
}

#[test]
fn rgxall_multi_no_match_returns_empty() {
    check(r#"f>L t;rgxall-multi ["\d+"] "no digits here""#, "[]");
}

#[test]
fn rgxall_multi_single_captureless_pattern() {
    // Single pattern, no capture groups — whole matches.
    check(r#"f>L t;rgxall-multi ["\d+"] "a1 b22 c333""#, "[1, 22, 333]");
}

#[test]
fn rgxall_multi_single_capture_pattern() {
    // Single pattern, 1 capture group — capture-1 strings.
    check(
        r#"f>L t;rgxall-multi ["<h2>([^<]+)</h2>"] "<h2>One</h2><h2>Two</h2>""#,
        "[One, Two]",
    );
}

#[test]
fn rgxall_multi_two_patterns_concat_in_order() {
    // Two patterns: digits first, then single-capture key names.
    check(
        r#"f>L t;rgxall-multi ["\d+" "([a-z]+)="] "errors=3 retries=1""#,
        "[3, 1, errors, retries]",
    );
}

#[test]
fn rgxall_multi_mixed_captureless_and_capture() {
    // Mix a captureless and a capture pattern.
    check(
        r#"f>L t;rgxall-multi ["\d+" "([A-Z]+)"] "ERR code=42 WARN count=7""#,
        "[42, 7, ERR, WARN]",
    );
}

#[test]
fn rgxall_multi_three_patterns_log_line() {
    // Log-line shape from cron-explainer persona.
    check(
        r#"f>L t;rgxall-multi ["\d{4}-\d{2}-\d{2}" "[A-Z]+" "\d+"] "2024-01-15 ERR n=5""#,
        "[2024-01-15, ERR, 2024, 01, 15, 5]",
    );
}

#[test]
fn rgxall_multi_pattern_with_no_hit_in_middle_is_skipped() {
    // Second pattern matches nothing — result is concatenation of pat1 and pat3.
    check(
        r#"f>L t;rgxall-multi ["\d+" "[A-Z]+" "\d+"] "abc 1 def 2""#,
        "[1, 2, 1, 2]",
    );
}

// ---- equivalence with flat+map+rgxall1 ----

#[test]
fn rgxall_multi_equivalent_to_flat_map_rgxall1() {
    // rgxall-multi should give identical output to the verbose workaround.
    let shorthand = r#"f>L t;rgxall-multi ["\d+" "([a-z]+)"] "x1 y22 abc def""#;
    let verbose = r#"f>L t;flat (map (p:t>L t;rgxall1 p "x1 y22 abc def") ["\d+" "([a-z]+)"])"#;
    for engine in ENGINES {
        let a = run(shorthand, engine);
        let b = run(verbose, engine);
        assert_eq!(a, b, "engine={engine}: shorthand and verbose forms diverge");
    }
}

// ---- error cases ----

#[test]
fn rgxall_multi_non_list_first_arg_errors() {
    check_error(
        r#"f>L t;rgxall-multi "\d+" "input""#,
        "rgxall-multi",
    );
}

#[test]
fn rgxall_multi_non_text_pattern_in_list_errors() {
    check_error(
        r#"f>L t;rgxall-multi [1 "\d+"] "input""#,
        "rgxall-multi",
    );
}

#[test]
fn rgxall_multi_invalid_regex_errors() {
    check_error(
        r#"f>L t;rgxall-multi ["(unclosed"] "input""#,
        "rgxall-multi",
    );
}

#[test]
fn rgxall_multi_two_group_pattern_errors_with_hint() {
    // Pattern with 2 capture groups should error with a message pointing at rgxall.
    check_error(
        r#"f>L t;rgxall-multi ["(\w+)=(\d+)"] "x=1""#,
        "rgxall",
    );
}
