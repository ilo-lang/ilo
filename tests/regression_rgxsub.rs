// Cross-engine regression tests for the `rgxsub` builtin.
// rgxsub pattern replacement subject — global regex replace, returns text.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_text(engine: &str, src: &str) -> String {
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

fn check_all(src: &str, expected: &str) {
    for engine in ["--run-tree", "--run-vm"] {
        let actual = run_text(engine, src);
        assert_eq!(
            actual, expected,
            "engine={engine} src=`{src}`: got `{actual}`, expected `{expected}`"
        );
    }
    #[cfg(feature = "cranelift")]
    {
        let actual = run_text("--run-cranelift", src);
        assert_eq!(
            actual, expected,
            "engine=cranelift src=`{src}`: got `{actual}`, expected `{expected}`"
        );
    }
}

#[test]
fn rgxsub_literal_replace() {
    check_all(r#"f>t;rgxsub "foo" "bar" "foo foo foo""#, "bar bar bar");
}

#[test]
fn rgxsub_digits_to_x() {
    check_all(r#"f>t;rgxsub "\d+" "X" "a1 b22 c333""#, "aX bX cX");
}

#[test]
fn rgxsub_backrefs_swap() {
    check_all(
        r#"f>t;rgxsub "(\w+)\s+(\w+)" "$2 $1" "hello world""#,
        "world hello",
    );
}

#[test]
fn rgxsub_no_match_returns_subject() {
    check_all(r#"f>t;rgxsub "\d+" "X" "no digits here""#, "no digits here");
}

#[test]
fn rgxsub_empty_replacement_deletes() {
    check_all(r#"f>t;rgxsub "\s+" "" "a b   c\td""#, "abcd");
}
