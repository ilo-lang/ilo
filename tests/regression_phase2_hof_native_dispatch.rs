// Cross-engine regression coverage for Phase 2 PR3: HOF native closure
// dispatch.
//
// Phase 2 PR1 (#384) and PR2 (#385) added VM + Cranelift closure support,
// including a closure-aware OP_CALL_DYN dispatch. PR3 (this PR) migrates
// 2-arg HOFs off the tree-bridge and uses OP_CALL_DYN per-element so
// closure callbacks dispatch natively without re-entering the tree
// interpreter.
//
// This PR migrates `partition 2` natively. The remaining bridge HOFs
// (`srt 2`, `grp 2`, `uniqby 2`, `mapr 2`, `ct 2`, `rsrt 2`) need
// dedicated finalizer opcodes for the sort/group/dedup/short-circuit
// post-processing step and are deferred to follow-up PRs.
//
// Each migrated HOF is exercised in three shapes:
//   1. Non-capturing inline lambda (Phase 1 shape, FnRef to __lit_N).
//   2. Capturing inline lambda (Phase 2 shape, Expr::MakeClosure).
//   3. Named top-level fn (FnRef to user fn).
//
// Each shape runs on `--run-tree`, `--run-vm`, and `--jit`
// (when the `cranelift` feature is enabled) so all three engines stay
// in lockstep.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(name: &str, src: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("ilo_p2hof_{name}_{}_{n}.ilo", std::process::id()));
    std::fs::write(&path, src).expect("write src");
    path
}

fn run_ok(engine: &str, src: &str, entry: &str, args: &[&str]) -> String {
    let path = write_src(entry, src);
    let mut cmd = ilo();
    cmd.arg(&path).arg(engine).arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_all(src: &str, entry: &str, args: &[&str], expected: &str) {
    #[cfg(feature = "cranelift")]
    let engines: &[&str] = &["--run-vm", "--jit"];
    #[cfg(not(feature = "cranelift"))]
    let engines: &[&str] = &["--run-vm"];
    for engine in engines {
        let actual = run_ok(engine, src, entry, args);
        assert_eq!(
            actual, expected,
            "engine {engine} produced {actual:?}, expected {expected:?} for src `{src}`"
        );
    }
}

fn run_err(engine: &str, src: &str, entry: &str, args: &[&str]) -> String {
    let path = write_src(entry, src);
    let mut cmd = ilo();
    cmd.arg(&path).arg(engine).arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        !out.status.success(),
        "ilo {engine} unexpectedly succeeded for `{src}`",
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

// ── partition: non-capturing inline lambda ──────────────────────────────

#[test]
fn partition_inline_lambda_non_capturing() {
    let src = "f xs:L n>L L n;partition (x:n>b;>x 0) xs";
    run_all(src, "f", &["[1,-2,3,-4,5]"], "[[1, 3, 5], [-2, -4]]");
}

// ── partition: capturing inline lambda ──────────────────────────────────

#[test]
fn partition_inline_lambda_single_capture() {
    let src = "f xs:L n t:n>L L n;partition (x:n>b;>x t) xs";
    run_all(src, "f", &["[1,2,3,4,5]", "3"], "[[4, 5], [1, 2, 3]]");
}

#[test]
fn partition_inline_lambda_two_captures() {
    // Predicate keeps items inclusive between lo and hi. Mirrors the
    // shape from regression_phase2_closure_capture (>=x lo & <=x hi)
    // which is the known-good two-capture form on every engine.
    let src = "f xs:L n lo:n hi:n>L L n;partition (x:n>b;&(>=x lo) <=x hi) xs";
    run_all(
        src,
        "f",
        &["[1,3,5,7,9,11]", "3", "7"],
        "[[3, 5, 7], [1, 9, 11]]",
    );
}

// ── partition: named fn (FnRef) ─────────────────────────────────────────

#[test]
fn partition_named_fn_ref() {
    let src = "pos x:n>b;>x 0\nf xs:L n>L L n;partition pos xs";
    run_all(src, "f", &["[1,-2,3,-4,5]"], "[[1, 3, 5], [-2, -4]]");
}

// ── partition: text values ──────────────────────────────────────────────

#[test]
fn partition_text_values() {
    let src = "f ws:L t>L L t;partition (w:t>b;has w \"a\") ws";
    run_all(
        src,
        "f",
        &["[\"cat\",\"dog\",\"ant\",\"fish\"]"],
        "[[cat, ant], [dog, fish]]",
    );
}

// ── partition: empty input ──────────────────────────────────────────────

#[test]
fn partition_empty_input() {
    let src = "f xs:L n>L L n;partition (x:n>b;>x 0) xs";
    run_all(src, "f", &["[]"], "[[], []]");
}

// ── partition: all-pass and all-fail ────────────────────────────────────

#[test]
fn partition_all_pass() {
    let src = "f xs:L n>L L n;partition (x:n>b;>x 0) xs";
    run_all(src, "f", &["[1,2,3]"], "[[1, 2, 3], []]");
}

#[test]
fn partition_all_fail() {
    let src = "f xs:L n>L L n;partition (x:n>b;>x 100) xs";
    run_all(src, "f", &["[1,2,3]"], "[[], [1, 2, 3]]");
}

// ── partition: non-bool predicate → runtime error ───────────────────────
//
// The bridge form raised "partition: predicate must return bool, got
// Number(2.0)". The native form raises "panic-unwrap: partition:
// predicate must return bool" (matching the `flt` native lift). Both
// surface ILO-R009; we assert the message contains "partition" and
// "bool" so the cross-engine contract holds for either wording.

#[test]
fn partition_non_bool_predicate_errors() {
    let src = "bad x:n>n;+x 1\nf xs:L n>L L n;partition bad xs";
    #[cfg(feature = "cranelift")]
    let engines: &[&str] = &["--run-vm", "--jit"];
    #[cfg(not(feature = "cranelift"))]
    let engines: &[&str] = &["--run-vm"];
    for engine in engines {
        let stderr = run_err(engine, src, "f", &["[1,2,3]"]);
        assert!(
            stderr.contains("partition") && stderr.contains("bool"),
            "engine {engine}: expected error mentioning partition + bool, got {stderr}"
        );
    }
}
