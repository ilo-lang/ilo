// Cross-engine regression coverage for the path-manipulation builtins
// (`dirname`, `basename`, `pathjoin`) shipped in 0.12.1.
//
// These are pure-text Unix forward-slash operations with POSIX dirname /
// basename semantics and a list-form pathjoin (variadic-position was
// rejected to avoid the ILO-P101 arity-inference trap that `fmt` lives with;
// see ilo_assessment_feedback.md "Add path-manipulation builtins").
//
// The bridge contract: identical output on tree, VM, and Cranelift for every
// shape. Native lowering can graduate any of these off the tree bridge later
// without changing user-visible behaviour — but until then, this file is
// the pin.
//
// Edge cases covered (one test each, named after the row in SPEC.md):
//   dirname: "/a/b/c.txt", "a/b/c.txt", "/", "foo.txt" (-> ""), "foo/",
//            "/a" (-> "/"), "" (-> "").
//   basename: "/a/b/c.txt", "a/b/c.txt", "/", "foo.txt", "foo/" (-> "foo"),
//             "" (-> "").
//   pathjoin: [] (-> ""), ["foo"], ["a", "b", "c.txt"], joint-slash dedup
//             across the three positional variants, ["", "a", ""] dropping
//             empties, ["/", "a"] preserving the leading absolute root.
// Round-trip: pathjoin [dirname p, basename p] for a typical path returns
//             the original, which is the property monorepo-analyst rerun11
//             needed (see assessment doc).

use std::process::Command;

const ENGINES: &[&str] = &["--run-vm", "--jit"];

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_engine(src: &str, engine: &str) -> String {
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
        let actual = run_engine(src, engine);
        assert_eq!(
            actual, expected,
            "engine={engine}, src=`{src}`: got `{actual}`, expected `{expected}`"
        );
    }
}

// ── dirname ────────────────────────────────────────────────────────────

#[test]
fn dirname_absolute_path() {
    check(r#"f>t;dirname "/a/b/c.txt""#, "/a/b");
}

#[test]
fn dirname_relative_path() {
    check(r#"f>t;dirname "a/b/c.txt""#, "a/b");
}

#[test]
fn dirname_root_is_self() {
    // POSIX: dirname "/" -> "/".
    check(r#"f>t;dirname "/""#, "/");
}

#[test]
fn dirname_no_dir_component_is_empty() {
    // POSIX returns "." here; ilo returns "" so cat [dirname p, basename p]
    // round-trips a plain filename without injecting a phantom `./` prefix.
    check(r#"f>t;dirname "foo.txt""#, "");
}

#[test]
fn dirname_trailing_slash() {
    // "foo/" is the entry `foo` with no directory component — same as
    // `dirname "foo"` which returns "" (POSIX would say "."; ilo returns
    // "" so cat-round-trip stays clean).
    check(r#"f>t;dirname "foo/""#, "");
}

#[test]
fn dirname_single_absolute_component() {
    // `/a` -> `/` (POSIX).
    check(r#"f>t;dirname "/a""#, "/");
}

#[test]
fn dirname_empty_is_empty() {
    check(r#"f>t;dirname """#, "");
}

// ── basename ───────────────────────────────────────────────────────────

#[test]
fn basename_absolute_path() {
    check(r#"f>t;basename "/a/b/c.txt""#, "c.txt");
}

#[test]
fn basename_relative_path() {
    check(r#"f>t;basename "a/b/c.txt""#, "c.txt");
}

#[test]
fn basename_root_is_self() {
    check(r#"f>t;basename "/""#, "/");
}

#[test]
fn basename_no_dir_component_is_identity() {
    check(r#"f>t;basename "foo.txt""#, "foo.txt");
}

#[test]
fn basename_trailing_slash_stripped() {
    check(r#"f>t;basename "foo/""#, "foo");
}

#[test]
fn basename_empty_is_empty() {
    check(r#"f>t;basename """#, "");
}

// ── pathjoin ───────────────────────────────────────────────────────────

#[test]
fn pathjoin_empty_list_is_empty() {
    check(r#"f>t;pathjoin []"#, "");
}

#[test]
fn pathjoin_single_segment_is_identity() {
    check(r#"f>t;pathjoin ["foo"]"#, "foo");
}

#[test]
fn pathjoin_three_clean_segments() {
    check(r#"f>t;pathjoin ["a" "b" "c.txt"]"#, "a/b/c.txt");
}

#[test]
fn pathjoin_dedupes_trailing_then_leading_slashes() {
    // The full joint-slash table: trailing on a, leading on b, both.
    check(r#"f>t;pathjoin ["a/" "b"]"#, "a/b");
    check(r#"f>t;pathjoin ["a" "/b"]"#, "a/b");
    check(r#"f>t;pathjoin ["a/" "/b"]"#, "a/b");
}

#[test]
fn pathjoin_drops_empty_segments() {
    check(r#"f>t;pathjoin ["" "a" ""]"#, "a");
}

#[test]
fn pathjoin_preserves_absolute_root() {
    // `["/", "a"]` keeps the leading `/` (not collapsed by the joint-dedup).
    check(r#"f>t;pathjoin ["/" "a"]"#, "/a");
}

#[test]
fn pathjoin_full_absolute_with_intermediate_slashes() {
    // The shape monorepo-analyst rerun11 needed: rebuild an absolute path
    // from base + sub-path segments without manually counting slashes.
    check(r#"f>t;pathjoin ["/a/" "/b/" "c.txt"]"#, "/a/b/c.txt");
}

// ── round-trip ─────────────────────────────────────────────────────────

#[test]
fn dirname_basename_round_trip_via_pathjoin() {
    // The property that justifies returning "" (not ".") from dirname on a
    // plain filename: cat [dirname p, basename p] joined by "/" must equal
    // the original path for the dominant agent shape (a path with at least
    // one directory component).
    check(
        r#"f>t;pathjoin [dirname "/a/b/c.txt" basename "/a/b/c.txt"]"#,
        "/a/b/c.txt",
    );
}

#[test]
fn dirname_basename_round_trip_relative() {
    check(
        r#"f>t;pathjoin [dirname "a/b/c.txt" basename "a/b/c.txt"]"#,
        "a/b/c.txt",
    );
}
