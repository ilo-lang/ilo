// Cross-engine regression tests for the filesystem-metadata builtins
// `fsize`, `mtime`, `isfile`, `isdir`. Like the `lsd`/`walk`/`glob` family,
// these are tree-bridge eligible (see `is_tree_bridge_eligible` in
// src/vm/mod.rs) — the tree interpreter is the dispatch target for every
// engine. Past regressions have hidden in the bridge round-trip (NanVal
// boxing, Result auto-unwrap, bool tag handling), so each assertion runs
// against tree, VM, and Cranelift JIT.
//
// Shape contract under test:
//   fsize  path > R n t  (Err on missing / perm-denied / dir)
//   mtime  path > R n t  (Unix epoch seconds, f64 — fractional preserved)
//   isfile path > b      (false on missing / perm-denied / dir / not-file)
//   isdir  path > b      (false on missing / perm-denied / not-dir)
//
// Predicate asymmetry vs size/mtime is deliberate (matches Python). The
// predicates collapse the error tier into `false` so `?isfile p{...}` is
// one token wide; size and mtime expose the error tier because the caller
// usually wants to distinguish missing from permission-denied from a
// path-is-directory category-error.

use std::fs;
use std::process::Command;
use tempfile::tempdir;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--run-vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--run-vm"];

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
    String::from_utf8_lossy(&out.stderr)
        .trim_end_matches('\n')
        .to_string()
}

// --- fsize ---------------------------------------------------------------

#[test]
fn fsize_basic_cross_engine() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("hello.txt");
    fs::write(&path, b"hello").unwrap();
    let src = "f p:t>R n t;fsize p";
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f", path.to_str().unwrap()]);
        assert_eq!(out, "5", "{engine}: fsize 5 bytes");
    }
}

#[test]
fn fsize_zero_byte_file_cross_engine() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("empty");
    fs::write(&path, b"").unwrap();
    let src = "f p:t>R n t;fsize p";
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f", path.to_str().unwrap()]);
        assert_eq!(out, "0", "{engine}: fsize empty file");
    }
}

#[test]
fn fsize_missing_is_err_cross_engine() {
    let src = "f p:t>R n t;fsize p";
    for engine in ENGINES_ALL {
        let out = run_err(engine, src, &["f", "/no/such/path/xyzzy-ilo-fsize"]);
        assert!(
            out.starts_with('^'),
            "{engine}: expected ^err on missing, got {out:?}"
        );
    }
}

#[test]
fn fsize_on_directory_is_err_cross_engine() {
    let dir = tempdir().unwrap();
    let src = "f p:t>R n t;fsize p";
    for engine in ENGINES_ALL {
        let out = run_err(engine, src, &["f", dir.path().to_str().unwrap()]);
        assert!(
            out.starts_with('^'),
            "{engine}: expected ^err on dir, got {out:?}"
        );
        assert!(
            out.contains("is a directory"),
            "{engine}: expected directory message, got {out:?}"
        );
    }
}

// --- mtime ---------------------------------------------------------------

#[test]
fn mtime_basic_cross_engine() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("ts.txt");
    fs::write(&path, b"x").unwrap();
    let src = "f p:t>R n t;mtime p";
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f", path.to_str().unwrap()]);
        // Epoch seconds, sanity-bound the value rather than asserting an
        // exact match — we only care the value is plausibly "now" (well
        // after the epoch and before the year-3000 wraparound).
        let v: f64 = out
            .parse()
            .unwrap_or_else(|_| panic!("{engine}: expected number, got {out:?}"));
        assert!(
            v > 1_700_000_000.0 && v < 32_000_000_000.0,
            "{engine}: mtime {v} out of sane bounds"
        );
    }
}

#[test]
fn mtime_missing_is_err_cross_engine() {
    let src = "f p:t>R n t;mtime p";
    for engine in ENGINES_ALL {
        let out = run_err(engine, src, &["f", "/no/such/path/xyzzy-ilo-mtime"]);
        assert!(
            out.starts_with('^'),
            "{engine}: expected ^err on missing, got {out:?}"
        );
    }
}

// --- isfile --------------------------------------------------------------

#[test]
fn isfile_true_on_regular_file_cross_engine() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, b"a").unwrap();
    let src = "f p:t>b;isfile p";
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f", path.to_str().unwrap()]);
        assert_eq!(out, "true", "{engine}: isfile on file");
    }
}

#[test]
fn isfile_false_on_directory_cross_engine() {
    let dir = tempdir().unwrap();
    let src = "f p:t>b;isfile p";
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f", dir.path().to_str().unwrap()]);
        assert_eq!(out, "false", "{engine}: isfile on dir");
    }
}

#[test]
fn isfile_false_on_missing_cross_engine() {
    let src = "f p:t>b;isfile p";
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f", "/no/such/path/xyzzy-ilo-isfile"]);
        assert_eq!(out, "false", "{engine}: isfile on missing");
    }
}

// --- isdir ---------------------------------------------------------------

#[test]
fn isdir_true_on_directory_cross_engine() {
    let dir = tempdir().unwrap();
    let src = "f p:t>b;isdir p";
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f", dir.path().to_str().unwrap()]);
        assert_eq!(out, "true", "{engine}: isdir on dir");
    }
}

#[test]
fn isdir_false_on_file_cross_engine() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, b"a").unwrap();
    let src = "f p:t>b;isdir p";
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f", path.to_str().unwrap()]);
        assert_eq!(out, "false", "{engine}: isdir on file");
    }
}

#[test]
fn isdir_false_on_missing_cross_engine() {
    let src = "f p:t>b;isdir p";
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f", "/no/such/path/xyzzy-ilo-isdir"]);
        assert_eq!(out, "false", "{engine}: isdir on missing");
    }
}

// --- symlink-follow behaviour -------------------------------------------
//
// All four builtins follow symlinks (POSIX `stat`, not `lstat`). The
// rationale: `walk` and `glob` don't follow symlinks for cycle-safety, but
// metadata calls are leaf queries where the agent almost always wants
// "what does this path resolve to?" not "what is this entry literally?".

#[cfg(unix)]
#[test]
fn isfile_follows_symlink_to_file_cross_engine() {
    use std::os::unix::fs::symlink;
    let dir = tempdir().unwrap();
    let target = dir.path().join("target.txt");
    fs::write(&target, b"x").unwrap();
    let link = dir.path().join("link");
    symlink(&target, &link).unwrap();

    let src = "f p:t>b;isfile p";
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f", link.to_str().unwrap()]);
        assert_eq!(out, "true", "{engine}: isfile through symlink to file");
    }
}

#[cfg(unix)]
#[test]
fn fsize_follows_symlink_to_file_cross_engine() {
    use std::os::unix::fs::symlink;
    let dir = tempdir().unwrap();
    let target = dir.path().join("target.txt");
    fs::write(&target, b"hello").unwrap();
    let link = dir.path().join("link");
    symlink(&target, &link).unwrap();

    let src = "f p:t>R n t;fsize p";
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f", link.to_str().unwrap()]);
        assert_eq!(out, "5", "{engine}: fsize through symlink");
    }
}

// --- composition smoke ---------------------------------------------------
//
// The predicates exist to compose with `lsd`/`walk` output via `flt`. This
// smoke test pins that the bridge round-trips fine when `isfile` is used
// inside a higher-order context (the most common real call site).

#[test]
fn isfile_in_flt_cross_engine() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("a.txt"), "a").unwrap();
    fs::write(root.join("b.txt"), "b").unwrap();
    fs::create_dir(root.join("sub")).unwrap();

    // walk returns relative paths, so we need to re-absolute them before
    // calling isfile. cat with the dir prefix is the canonical pattern.
    let src = "f d:t>L t;\
        xs=walk! d;\
        flt (p:t>b;isfile cat [d \"/\" p] \"\") xs";
    for engine in ENGINES_ALL {
        let out = run_ok(engine, src, &["f", root.to_str().unwrap()]);
        // Order is deterministic from walk (lexicographic): a.txt, b.txt,
        // then sub which is a dir and is filtered out.
        assert_eq!(out, "[a.txt, b.txt]", "{engine}: isfile inside flt");
    }
}
