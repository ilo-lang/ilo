// Cross-engine regression tests for the filesystem-enumeration builtins
// `lsd`, `walk`, and `glob`. These are tree-bridge eligible (see
// `is_tree_bridge_eligible` in src/vm/mod.rs), so the same interpreter path
// services every engine — but the bridge is exactly where past regressions
// have hidden, so every assertion runs against tree, VM, and Cranelift JIT.
//
// Fixture layout (created per-test in a tempdir so tests don't share state):
//
//   <root>/a.txt
//   <root>/b.txt
//   <root>/sub/c.txt
//   <root>/sub/d.log
//   <root>/sub/nested/e.txt
//
// We rely on `tempfile` which is already a dev-dependency for the suite.

use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm"];

fn make_fixture() -> tempfile::TempDir {
    let dir = tempdir().expect("tempdir");
    let root = dir.path();
    fs::write(root.join("a.txt"), "a").unwrap();
    fs::write(root.join("b.txt"), "b").unwrap();
    fs::create_dir(root.join("sub")).unwrap();
    fs::write(root.join("sub").join("c.txt"), "c").unwrap();
    fs::write(root.join("sub").join("d.log"), "d").unwrap();
    fs::create_dir(root.join("sub").join("nested")).unwrap();
    fs::write(root.join("sub").join("nested").join("e.txt"), "e").unwrap();
    dir
}

fn run_ok(engine: &str, src: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} {src:?} {args:?} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .trim_end_matches('\n')
        .to_string()
}

fn run_err(engine: &str, src: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} {src:?} {args:?} unexpectedly succeeded: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    // Top-level Err on a Result-returning fn prints to stderr with `^` prefix
    // and exits 1 (in-process AOT-equivalent contract). Stdout is empty in
    // that path, so the err-shape assertion reads stderr.
    let s = String::from_utf8_lossy(&out.stderr);
    s.trim_end_matches('\n').to_string()
}

/// `lsd dir` returns only the filenames (no path prefix), sorted, and includes
/// directory entries alongside file entries — both `a.txt` and `sub` appear.
#[test]
fn ls_basic_cross_engine() {
    let fix = make_fixture();
    let root = fix.path().to_str().unwrap();
    let src = "f d:t>R (L t) t;lsd d";
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f", root]);
        assert_eq!(out, "[a.txt, b.txt, sub]", "{engine}: ls basic");
    }
}

/// `lsd` on an empty directory returns an empty list, not an error.
#[test]
fn ls_empty_dir_cross_engine() {
    let dir = tempdir().unwrap();
    let root = dir.path().to_str().unwrap();
    let src = "f d:t>R (L t) t;lsd d";
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f", root]);
        assert_eq!(out, "[]", "{engine}: ls empty");
    }
}

/// Missing directory surfaces an `Err` value rather than a runtime panic.
/// The bang-unwrap in `main` propagates it, so stdout carries `^...` and
/// the exit code is non-zero — same shape as `rd` on a missing file.
#[test]
fn ls_missing_dir_cross_engine() {
    let src = "f d:t>R (L t) t;lsd d";
    for engine in ENGINES_ALL {
        let out = run_err(
            engine,
            src,
            &["f", "/this/path/should/never/exist/xyzzy-ilo"],
        );
        assert!(
            out.starts_with('^'),
            "{engine}: expected ^err prefix, got {out:?}"
        );
    }
}

/// `walk dir` recurses and returns relative paths, sorted lexicographically.
/// Directory entries are included alongside file entries so the agent can
/// distinguish prefixes from terminals (matches `find <dir>` shape).
#[test]
fn walk_recursive_cross_engine() {
    let fix = make_fixture();
    let root = fix.path().to_str().unwrap();
    let src = "f d:t>R (L t) t;walk d";
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f", root]);
        // Sort order is lexicographic on the relative paths.
        assert_eq!(
            out, "[a.txt, b.txt, sub, sub/c.txt, sub/d.log, sub/nested, sub/nested/e.txt]",
            "{engine}: walk recursive"
        );
    }
}

/// `walk` on a non-existent root surfaces as Err.
#[test]
fn walk_missing_dir_cross_engine() {
    let src = "f d:t>R (L t) t;walk d";
    for engine in ENGINES_ALL {
        let out = run_err(
            engine,
            src,
            &["f", "/this/path/should/never/exist/xyzzy-ilo"],
        );
        assert!(
            out.starts_with('^'),
            "{engine}: expected ^err prefix, got {out:?}"
        );
    }
}

/// `glob dir "*.txt"` matches files in the immediate directory only.
/// `*` does not cross path separators by design.
#[test]
fn glob_star_single_segment_cross_engine() {
    let fix = make_fixture();
    let root = fix.path().to_str().unwrap();
    let src = "f d:t p:t>R (L t) t;glob d p";
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f", root, "*.txt"]);
        assert_eq!(out, "[a.txt, b.txt]", "{engine}: glob *.txt");
    }
}

/// `glob dir "**/*.txt"` matches recursively across path separators.
/// Verifies the `**` operator pulls in nested matches AND that the top-level
/// matches still appear (zero-segment match for the `**`).
#[test]
fn glob_double_star_recursive_cross_engine() {
    let fix = make_fixture();
    let root = fix.path().to_str().unwrap();
    let src = "f d:t p:t>R (L t) t;glob d p";
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f", root, "**/*.txt"]);
        // Sorted lexicographically; nested e.txt appears, d.log does not.
        assert_eq!(
            out, "[a.txt, b.txt, sub/c.txt, sub/nested/e.txt]",
            "{engine}: glob **/*.txt"
        );
    }
}

/// Character class `[ab]` matches a single char from the class.
#[test]
fn glob_char_class_cross_engine() {
    let fix = make_fixture();
    let root = fix.path().to_str().unwrap();
    let src = "f d:t p:t>R (L t) t;glob d p";
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f", root, "[ab].txt"]);
        assert_eq!(out, "[a.txt, b.txt]", "{engine}: glob [ab].txt");
    }
}

/// `glob` on a non-existent root surfaces as Err — same shape as `walk`.
#[test]
fn glob_missing_dir_cross_engine() {
    let src = "f d:t p:t>R (L t) t;glob d p";
    for engine in ENGINES_ALL {
        let out = run_err(
            engine,
            src,
            &["f", "/this/path/should/never/exist/xyzzy-ilo", "*"],
        );
        assert!(
            out.starts_with('^'),
            "{engine}: expected ^err prefix, got {out:?}"
        );
    }
}

/// glob_match unit-level behaviour reachable through the engine: a pattern
/// with no matches returns an empty list (no Err).
#[test]
fn glob_no_matches_returns_empty_cross_engine() {
    let fix = make_fixture();
    let root = fix.path().to_str().unwrap();
    let src = "f d:t p:t>R (L t) t;glob d p";
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f", root, "no-such-pattern-*.xyzzy"]);
        assert_eq!(out, "[]", "{engine}: glob empty match");
    }
}

/// Smoke: walk into a directory containing only one nested file works without
/// off-by-one issues (regression for an early hand-rolled DFS that wrongly
/// pushed the file as a directory).
#[test]
fn walk_single_file_cross_engine() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("only.txt"), "x").unwrap();
    let src = "f d:t>R (L t) t;walk d";
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f", dir.path().to_str().unwrap()]);
        assert_eq!(out, "[only.txt]", "{engine}: walk single file");
    }
}
