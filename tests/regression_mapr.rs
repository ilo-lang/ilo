// Cross-engine regression tests for the `mapr` HOF (short-circuiting
// Result-aware map). Pins behaviour across tree / VM / Cranelift, mirroring
// the regression_hof_3b.rs shape used for the grp/uniqby/partition/srt
// tree-bridge family.
//
// `mapr fn xs` calls fn on each element, accumulates the inner Ok values
// on the all-Ok path (returning `~(L b)`), and short-circuits on the first
// Err (returning `^e` without visiting the tail). VM and Cranelift route
// through the tree-bridge (`is_tree_bridge_eligible(Mapr, 2)`), so the
// callback dispatch is the same code path as grp/uniqby/partition/srt.
//
// Originating friction: ilo_assessment_feedback.md line 2541 (html-scraper
// rerun3, persona kept writing a `ton s:t>n;r=num s;?r{~v:v;^_:0}` helper
// because `map num xs` returns `L (R n t)` with no clean unwrap path).

use std::process::Command;

const ENGINES: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_ok(src: &str, engine: &str, entry: &str, extra: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine).arg(entry);
    for a in extra {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}` entry `{entry}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(src: &str, engine: &str, entry: &str, extra: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine).arg(entry);
    for a in extra {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} unexpectedly succeeded for `{src}` entry `{entry}`: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn check_ok(src: &str, entry: &str, extra: &[&str], expected: &str) {
    for engine in ENGINES {
        let actual = run_ok(src, engine, entry, extra);
        assert_eq!(
            actual, expected,
            "engine={engine}, src=`{src}`, entry=`{entry}` extra={extra:?}: got `{actual}`, expected `{expected}`"
        );
    }
}

fn check_err_contains(src: &str, entry: &str, extra: &[&str], needle: &str) {
    for engine in ENGINES {
        let stderr = run_err(src, engine, entry, extra);
        assert!(
            stderr.contains(needle),
            "engine={engine}: stderr missing `{needle}`: {stderr}"
        );
    }
}

// ── happy path: every string parses cleanly, returns ~[1,2,3] ─────────────

#[test]
fn mapr_all_ok_returns_inner_list() {
    check_ok(
        r#"f>R (L n) t;mapr num ["1","2","3"]"#,
        "f",
        &[],
        "[1, 2, 3]",
    );
}

// ── empty list: trivially ~[] ─────────────────────────────────────────────

#[test]
fn mapr_empty_list_returns_empty_ok() {
    check_ok(r#"f>R (L n) t;mapr num []"#, "f", &[], "[]");
}

// ── err at head: short-circuit, never visit tail ─────────────────────────

#[test]
fn mapr_first_err_short_circuits() {
    check_err_contains(r#"f>R (L n) t;mapr num ["bad","2","3"]"#, "f", &[], "^bad");
}

// ── err mid-list: same short-circuit, but the err comes from the middle ──

#[test]
fn mapr_mid_err_short_circuits() {
    check_err_contains(r#"f>R (L n) t;mapr num ["1","bad","3"]"#, "f", &[], "^bad");
}

// ── err at tail: same shape ──────────────────────────────────────────────

#[test]
fn mapr_tail_err_short_circuits() {
    check_err_contains(r#"f>R (L n) t;mapr num ["1","2","bad"]"#, "f", &[], "^bad");
}

// ── `!` auto-unwrap threads the err up into the caller ───────────────────
//
// Bare `mapr fn xs` returns R; pairing with `!` extracts the inner list
// inside the body, and the err short-circuit propagates out of the
// surrounding function (which must also return R for `!` to typecheck).

#[test]
fn mapr_bang_ok_propagates_to_caller() {
    check_ok(
        r#"count xs:L t>R n t;ns=mapr! num xs;~len ns"#,
        "count",
        &[r#"["1","2","3"]"#],
        "3",
    );
}

#[test]
fn mapr_bang_err_propagates_to_caller() {
    check_err_contains(
        r#"count xs:L t>R n t;ns=mapr! num xs;~len ns"#,
        "count",
        &[r#"["1","bad","3"]"#],
        "^bad",
    );
}

// ── user-defined fn returning R: dispatched through the tree-bridge ──────
//
// This is the load-bearing piece for VM and Cranelift: the user fn must be
// resolvable from the tree-bridge via ACTIVE_AST_PROGRAM (the same path
// grp/uniqby/partition/srt take in PR 3b).

#[test]
fn mapr_user_fn_dispatch_cross_engine() {
    // safe-div returns ^"divzero" when the divisor is zero, else ~/100 d.
    // mapr accumulates the Ok divisions; any zero in the list bails.
    check_ok(
        r#"sd d:n>R n t;=d 0 ^"divzero";~/100 d
f>R (L n) t;mapr sd [2,4,5]"#,
        "f",
        &[],
        "[50, 25, 20]",
    );
    check_err_contains(
        r#"sd d:n>R n t;=d 0 ^"divzero";~/100 d
f>R (L n) t;mapr sd [2,0,5]"#,
        "f",
        &[],
        "^divzero",
    );
}

// ── verifier rejects non-fn first arg ─────────────────────────────────────

#[test]
fn mapr_non_fn_first_arg_rejected_cross_engine() {
    check_err_contains(
        r#"f>R (L n) t;mapr 42 ["1"]"#,
        "f",
        &[],
        "'mapr' first arg must be a function",
    );
}

// ── verifier rejects fn that doesn't return Result ───────────────────────
//
// `mapr` is for fallible fns; reaching for it with `dbl x:n>n` is a sign
// the caller wanted plain `map`. Verifier catches it before runtime so the
// agent gets a clear redirect instead of a downstream type error.

#[test]
fn mapr_non_result_fn_rejected_cross_engine() {
    check_err_contains(
        r#"dbl x:n>n;*x 2
f xs:L n>R (L n) t;mapr dbl xs"#,
        "f",
        &["[1,2]"],
        "'mapr' fn must return a Result",
    );
}

// ── arity rejection: 1-arg or 3-arg mapr is not (yet) accepted ───────────

#[test]
fn mapr_wrong_arity_rejected() {
    // 1 arg: missing list.
    check_err_contains(r#"f>R (L n) t;mapr num"#, "f", &[], "mapr");
}
