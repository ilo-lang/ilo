// Cross-engine regression coverage for inline lambdas (Phase 1) as
// HOF arguments, on every dispatch surface that landed in the recent
// HOF chain (#274 FnRef, #277 native map, #278 flt/fld/flatmap,
// #279 grp/uniqby/srt-2arg/partition, #280 closure-bind).
//
// Phase 1 inline lambdas lift `(params>ret;body)` literals to
// synthetic `__lit_N` top-level decls. With FnRef plumbing + HOF
// dispatch now working cross-engine, lambdas-as-args should produce
// identical results on tree, VM, and Cranelift for every HOF.
//
// Both 2-arg (no ctx) and 3-arg (ctx-bind) forms are exercised. The
// ctx form is how Phase 1 lambdas thread external state without
// closure capture; closure capture (Phase 2) is still tree-only, so
// these tests deliberately use the ctx form to keep coverage flat
// across every engine.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(name: &str, src: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "ilo_lambdas_xeng_{name}_{}_{n}.ilo",
        std::process::id()
    ));
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
    for engine in ["--run-tree", "--run-vm", "--run-cranelift"] {
        let actual = run_ok(engine, src, entry, args);
        assert_eq!(
            actual, expected,
            "engine {engine} produced {actual:?}, expected {expected:?} for src `{src}`"
        );
    }
}

// ── map ─────────────────────────────────────────────────────────────────

#[test]
fn lambda_map_2arg_square() {
    let src = "main xs:L n>L n;map (x:n>n;*x x) xs";
    run_all(src, "main", &["[1,2,3,4]"], "[1, 4, 9, 16]");
}

#[test]
fn lambda_map_3arg_ctx_bump() {
    // 3-arg form: ctx (`bump`) is passed as the second positional
    // and forwarded to the lambda as its trailing param. This pins
    // the closure-bind path (PR 3c, #280) for inline lambdas.
    let src = "main bump:n xs:L n>L n;map (x:n c:n>n;+x c) bump xs";
    run_all(src, "main", &["10", "[1,2,3]"], "[11, 12, 13]");
}

// ── flt ─────────────────────────────────────────────────────────────────

#[test]
fn lambda_flt_2arg_positive() {
    let src = "main xs:L n>L n;flt (x:n>b;>x 0) xs";
    run_all(src, "main", &["[-1,2,-3,4,-5,6]"], "[2, 4, 6]");
}

#[test]
fn lambda_flt_3arg_ctx_threshold() {
    let src = "main thr:n xs:L n>L n;flt (x:n c:n>b;>x c) thr xs";
    run_all(src, "main", &["3", "[1,2,3,4,5]"], "[4, 5]");
}

// ── fld ─────────────────────────────────────────────────────────────────

#[test]
fn lambda_fld_2arg_sum() {
    let src = "main xs:L n>n;fld (a:n x:n>n;+a x) xs 0";
    run_all(src, "main", &["[1,2,3,4,5]"], "15");
}

#[test]
fn lambda_fld_3arg_ctx_weighted_sum() {
    // ctx is the per-item weight, fld signature is (fn ctx xs seed).
    let src = "main w:n xs:L n>n;fld (a:n x:n c:n>n;+a *x c) w xs 0";
    run_all(src, "main", &["5", "[1,2,3,4]"], "50");
}

// ── srt ─────────────────────────────────────────────────────────────────

#[test]
fn lambda_srt_2arg_by_abs() {
    let src = "main xs:L n>L n;srt (x:n>n;abs x) xs";
    run_all(src, "main", &["[-3,1,-5,2]"], "[1, 2, -3, -5]");
}

#[test]
fn lambda_srt_3arg_ctx_distance() {
    // Sort by distance from a ctx target.
    let src = "main t:n xs:L n>L n;srt (x:n c:n>n;abs -x c) t xs";
    run_all(src, "main", &["8", "[1,5,10,20]"], "[10, 5, 1, 20]");
}

// ── grp ─────────────────────────────────────────────────────────────────

#[test]
fn lambda_grp_2arg_by_sign() {
    let src = "main xs:L n>M t (L n);grp (x:n>t;>x 0{\"pos\"}{\"np\"}) xs";
    run_all(src, "main", &["[-1,2,-3,4]"], "{np: [-1, -3]; pos: [2, 4]}");
}

// ── uniqby ──────────────────────────────────────────────────────────────

#[test]
fn lambda_uniqby_2arg_by_parity() {
    let src = "main xs:L n>L n;uniqby (x:n>n;mod x 2) xs";
    run_all(src, "main", &["[1,3,2,4,5,6]"], "[1, 2]");
}

// ── partition ───────────────────────────────────────────────────────────

#[test]
fn lambda_partition_2arg_positive() {
    let src = "main xs:L n>L (L n);partition (x:n>b;>x 0) xs";
    run_all(src, "main", &["[-1,2,-3,4]"], "[[2, 4], [-1, -3]]");
}

// ── flatmap ─────────────────────────────────────────────────────────────

#[test]
fn lambda_flatmap_2arg_duplicate_and_double() {
    // Each x emits [x, x*2]; result flattens one level.
    let src = "main xs:L n>L n;flatmap (x:n>L n;[x,*x 2]) xs";
    run_all(src, "main", &["[1,2,3]"], "[1, 2, 2, 4, 3, 6]");
}

// ── Empty-list coverage ─────────────────────────────────────────────────
//
// HOF dispatchers must short-circuit empty input before the first
// OP_CALL_DYN. Pin that for inline lambdas on every engine.

#[test]
fn lambda_map_empty_list() {
    let src = "main xs:L n>L n;map (x:n>n;*x x) xs";
    run_all(src, "main", &["[]"], "[]");
}

#[test]
fn lambda_flt_empty_list() {
    let src = "main xs:L n>L n;flt (x:n>b;>x 0) xs";
    run_all(src, "main", &["[]"], "[]");
}

#[test]
fn lambda_fld_empty_list_returns_seed() {
    let src = "main xs:L n>n;fld (a:n x:n>n;+a x) xs 7";
    run_all(src, "main", &["[]"], "7");
}
