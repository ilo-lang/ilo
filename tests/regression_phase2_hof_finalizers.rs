// Cross-engine regression coverage for Phase 2 PR3b: HOF finalizer
// opcodes.
//
// PR3 (#387) lifted `partition 2` off the tree-bridge. PR3b extends the
// migration to the remaining bridge HOFs that need post-loop finalization:
// `srt 2` / `grp 2` / `uniqby 2` / `mapr 2`. Each lift uses OP_CALL_DYN
// per element for closure-aware dispatch, then assembles the final result
// via either an existing opcode (mapr: OP_WRAPOK on the acc list) or a
// new dedicated finalizer opcode (srt/grp/uniqby: 179-181, follow-up PR).
//
// PR3b shipped the `mapr 2` lift. PR3c lifts the remaining three HOFs
// (`srt 2` / `grp 2` / `uniqby 2`) with dedicated finalizer opcodes
// (OP_SRT_BY_KEY / OP_GRP_BY_KEY / OP_UNIQ_BY_KEY). The shape is shared
// across all three: a foreach loop fills parallel keys + values lists
// via OP_CALL_DYN, then the finalizer opcode walks them in lockstep to
// assemble the final sorted list / grouped map / deduped list.
//
// Each migrated HOF is exercised in three shapes:
//   1. Non-capturing inline lambda (Phase 1 shape, FnRef to __lit_N).
//   2. Capturing inline lambda (Phase 2 shape, Expr::MakeClosure).
//   3. Named top-level fn (FnRef to user fn).
//
// Plus an empty-input case and (for mapr) a short-circuit case that
// returns Err mid-loop without producing a full result list.
//
// Each shape runs on `--run-tree`, `--run-vm`, and `--run-cranelift`
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
    path.push(format!("ilo_p3b_{name}_{}_{n}.ilo", std::process::id()));
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
    let engines: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];
    #[cfg(not(feature = "cranelift"))]
    let engines: &[&str] = &["--run-tree", "--run-vm"];
    for engine in engines {
        let actual = run_ok(engine, src, entry, args);
        assert_eq!(
            actual, expected,
            "engine {engine} produced {actual:?}, expected {expected:?} for src `{src}`"
        );
    }
}

fn run_err_combined(engine: &str, src: &str, entry: &str, args: &[&str]) -> (i32, String) {
    let path = write_src(entry, src);
    let mut cmd = ilo();
    cmd.arg(&path).arg(engine).arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    let code = out.status.code().unwrap_or(-1);
    // mapr short-circuits via an Err Result falling out of main; the CLI
    // unwraps and prints the inner message to STDERR (matches the
    // tree-walker's ILO-R009 path). We check stderr for the message.
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    (code, stderr)
}

fn run_err_all_contains(src: &str, entry: &str, args: &[&str], needle: &str) {
    #[cfg(feature = "cranelift")]
    let engines: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];
    #[cfg(not(feature = "cranelift"))]
    let engines: &[&str] = &["--run-tree", "--run-vm"];
    for engine in engines {
        let (code, stderr) = run_err_combined(engine, src, entry, args);
        assert_ne!(
            code, 0,
            "engine {engine}: expected non-zero exit for `{src}`"
        );
        assert!(
            stderr.contains(needle),
            "engine {engine}: expected stderr to contain `{needle}`, got `{stderr}`"
        );
    }
}

// ── mapr: non-capturing inline lambda ──────────────────────────────────

#[test]
fn mapr_inline_lambda_non_capturing() {
    let src = "f xs:L n>R (L n) t;mapr (x:n>R n t;~*x x) xs";
    run_all(src, "f", &["[1,2,3]"], "[1, 4, 9]");
}

// ── mapr: capturing inline lambda ──────────────────────────────────────

#[test]
fn mapr_inline_lambda_single_capture() {
    // Multiplier t is captured by the lambda; result is each item * t.
    let src = "f xs:L n t:n>R (L n) t;mapr (x:n>R n t;~*x t) xs";
    run_all(src, "f", &["[1,2,3]", "10"], "[10, 20, 30]");
}

// ── mapr: named fn (FnRef) ─────────────────────────────────────────────

#[test]
fn mapr_named_fn_ref() {
    let src = "sq x:n>R n t;~*x x\nf xs:L n>R (L n) t;mapr sq xs";
    run_all(src, "f", &["[2,3,4]"], "[4, 9, 16]");
}

// ── mapr: empty input ──────────────────────────────────────────────────

#[test]
fn mapr_empty_input() {
    let src = "sq x:n>R n t;~*x x\nf xs:L n>R (L n) t;mapr sq xs";
    run_all(src, "f", &["[]"], "[]");
}

// ── mapr: short-circuit on first Err ───────────────────────────────────
//
// Tree walker returns the Err from the first failing element and stops
// iterating. The native lift must match: per-element ISERR branches to
// out_reg = res_reg and skips the WRAPOK acc tail.
//
// The CLI prints an unwrapped Err Result as the inner message (no
// ^prefix in stdout since it falls out the top of `main` unwrapped).
// We assert via run_err_all_stdout (non-zero exit + the Err message).

#[test]
fn mapr_short_circuit_on_err() {
    let src = "chk x:n>R n t;>x 5 ^\"too big\";~*x 2\nf xs:L n>R (L n) t;mapr chk xs";
    run_err_all_contains(src, "f", &["[1,2,10,4]"], "too big");
}

#[test]
fn mapr_short_circuit_all_ok() {
    // Same predicate, but all items pass — no short-circuit, full result.
    let src = "chk x:n>R n t;>x 5 ^\"too big\";~*x 2\nf xs:L n>R (L n) t;mapr chk xs";
    run_all(src, "f", &["[1,2,3]"], "[2, 4, 6]");
}

// ── mapr: short-circuit on first element ───────────────────────────────
//
// Edge case: the very first element fails. The acc list is empty when
// we short-circuit, but we must still return the Err, not Ok([]).

#[test]
fn mapr_short_circuit_first_element() {
    let src = "chk x:n>R n t;>x 5 ^\"too big\";~*x 2\nf xs:L n>R (L n) t;mapr chk xs";
    run_err_all_contains(src, "f", &["[100,2,3]"], "too big");
}

// ── PR3c (srt 2 / grp 2 / uniqby 2 finalizer opcodes) ─────────────────
//
// Each HOF gets four shapes exercised across all engines: non-capturing
// inline lambda, capturing inline lambda, named fn-ref, and empty input.
// The non-capturing case uses an inline lambda whose body is a single
// op (the key function), so the bytecode hits OP_CALL_DYN against a
// FnRef-only NanVal. The capturing case bumps to OP_MAKE_CLOSURE +
// closure-aware OP_CALL_DYN — exercising the Phase 2 PR2 path under
// each new finalizer. The named-fn case ensures `srt fn xs` with a
// top-level fn works without any closure machinery.
//
// All assertions are against the tree-walker's canonical output, which
// the existing tree-bridge guaranteed before PR3c.

// ── srt 2 ─────────────────────────────────────────────────────────────

#[test]
fn srt_inline_lambda_non_capturing() {
    // Sort by absolute value (negated key path that tree-walker matches).
    let src = "k x:n>n;-0 x\nf xs:L n>L n;srt k xs";
    run_all(src, "f", &["[3,1,4,1,5,9,2,6]"], "[9, 6, 5, 4, 3, 2, 1, 1]");
}

#[test]
fn srt_inline_lambda_single_capture() {
    // Key is item*scale, so sort order depends on the captured scale.
    let src = "f xs:L n s:n>L n;srt (x:n>n;*x s) xs";
    run_all(src, "f", &["[3,1,4,1,5]", "1"], "[1, 1, 3, 4, 5]");
    // Negative scale flips the sort.
    run_all(src, "f", &["[3,1,4,1,5]", "-1"], "[5, 4, 3, 1, 1]");
}

#[test]
fn srt_named_fn_ref() {
    let src = "neg x:n>n;-0 x\nf xs:L n>L n;srt neg xs";
    run_all(src, "f", &["[2,8,5,1]"], "[8, 5, 2, 1]");
}

#[test]
fn srt_empty_input() {
    let src = "k x:n>n;-0 x\nf xs:L n>L n;srt k xs";
    run_all(src, "f", &["[]"], "[]");
}

// ── grp 2 ─────────────────────────────────────────────────────────────

#[test]
fn grp_inline_lambda_non_capturing() {
    // Group by "big" if > 5 else "small".
    let src = "k x:n>t;>x 5 \"big\";\"small\"\nf xs:L n>M t L n;grp k xs";
    run_all(
        src,
        "f",
        &["[1,2,7,3,8,9]"],
        "{big: [7, 8, 9]; small: [1, 2, 3]}",
    );
}

#[test]
fn grp_inline_lambda_single_capture() {
    // Threshold is captured, label by side of the threshold.
    let src = "f xs:L n t:n>M t L n;grp (x:n>t;>x t \"big\";\"small\") xs";
    run_all(
        src,
        "f",
        &["[1,2,7,3,8,9]", "5"],
        "{big: [7, 8, 9]; small: [1, 2, 3]}",
    );
}

#[test]
fn grp_named_fn_ref() {
    let src = "lbl x:n>t;>x 5 \"big\";\"small\"\nf xs:L n>M t L n;grp lbl xs";
    run_all(src, "f", &["[1,7,2,8]"], "{big: [7, 8]; small: [1, 2]}");
}

#[test]
fn grp_empty_input() {
    let src = "k x:n>t;>x 5 \"big\";\"small\"\nf xs:L n>M t L n;grp k xs";
    run_all(src, "f", &["[]"], "{}");
}

// ── uniqby 2 ──────────────────────────────────────────────────────────

#[test]
fn uniqby_inline_lambda_non_capturing() {
    // Dedup by "big" / "small" bucket — keep first per bucket.
    let src = "k x:n>t;>x 5 \"big\";\"small\"\nf xs:L n>L n;uniqby k xs";
    run_all(src, "f", &["[1,2,7,3,8,9]"], "[1, 7]");
}

#[test]
fn uniqby_inline_lambda_single_capture() {
    // Captured threshold drives the bucket.
    let src = "f xs:L n t:n>L n;uniqby (x:n>t;>x t \"big\";\"small\") xs";
    run_all(src, "f", &["[1,2,7,3,8,9]", "5"], "[1, 7]");
    // Tighter threshold flips first-seen on each side.
    run_all(src, "f", &["[1,2,7,3,8,9]", "2"], "[1, 7]");
}

#[test]
fn uniqby_named_fn_ref() {
    let src = "lbl x:n>t;>x 5 \"big\";\"small\"\nf xs:L n>L n;uniqby lbl xs";
    run_all(src, "f", &["[1,7,2,8]"], "[1, 7]");
}

#[test]
fn uniqby_empty_input() {
    let src = "k x:n>t;>x 5 \"big\";\"small\"\nf xs:L n>L n;uniqby k xs";
    run_all(src, "f", &["[]"], "[]");
}
