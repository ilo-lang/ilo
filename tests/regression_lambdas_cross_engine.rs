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

// ── Sequential HOFs in the same function ────────────────────────────────
//
// Regression: prior to the TLS save-restore fix in `fix/srt-cranelift-nil`,
// a tree-bridge HOF (srt-2arg / grp / uniqby / partition) running after a
// native-dispatch HOF (map / flt / fld / flatmap) in the same Cranelift
// entry returned `nil`. The native HOF's per-element callback re-entered
// the VM via `jit_call_dyn → VM::new(program).call(...)`; the inner
// execute()'s drop guards nulled `ACTIVE_FUNC_NAMES` / `ACTIVE_AST_PROGRAM`
// on return, so the second HOF's FnRef arg deserialised to a
// `<user_fn:N>` placeholder the tree-bridge couldn't dispatch, and the
// bridge swallowed the failure as TAG_NIL. Tree/VM were fine; only the
// Cranelift JIT path had the TLS desync. Pin every combination so this
// can't drift back.

#[test]
fn lambda_map_then_srt_no_tls_desync() {
    // Two inline lambdas in the same function body: one in `map`
    // (native dispatch), then one in `srt` (tree-bridge). On Cranelift
    // the second used to return nil; now must match tree/VM.
    let src =
        "main>L (L _);a=map (x:n>n;+x 1) [1 2 3];sk=srt (p:L _>n;p.0) [[2 \"a\"] [1 \"b\"]];sk";
    run_all(src, "main", &[], "[[1, b], [2, a]]");
}

#[test]
fn lambda_flt_then_srt_no_tls_desync() {
    let src =
        "main>L (L _);a=flt (x:n>b;>x 1) [1 2 3];sk=srt (p:L _>n;p.0) [[2 \"a\"] [1 \"b\"]];sk";
    run_all(src, "main", &[], "[[1, b], [2, a]]");
}

#[test]
fn lambda_fld_then_srt_no_tls_desync() {
    let src = "main>L (L _);s=fld (a:n x:n>n;+a x) [1 2 3] 0;sk=srt (p:L _>n;p.0) [[2 \"a\"] [1 \"b\"]];sk";
    run_all(src, "main", &[], "[[1, b], [2, a]]");
}

#[test]
fn lambda_flatmap_then_srt_no_tls_desync() {
    let src = "main>L (L _);a=flatmap (x:n>L n;[x x]) [1 2];sk=srt (p:L _>n;p.0) [[2 \"a\"] [1 \"b\"]];sk";
    run_all(src, "main", &[], "[[1, b], [2, a]]");
}

#[test]
fn lambda_map_then_srt_frq_mget_pairs() {
    // The original pdf-analyst rerun4 shape: frq + mget loop builds
    // `[count word]` pairs, then srt-by-count. Exercises the
    // tree-bridge for frq AND srt with an intervening native HOF
    // (map) earlier in the function body.
    let src = "main>L (L _);ws=[\"apple\" \"apple\" \"banana\" \"apple\" \"cherry\" \"banana\"];dummy=map (w:t>n;len w) ws;fr=frq ws;ks=mkeys fr;pairs=[];@k ks{c=mget fr k;pairs=+=pairs [c k]};sk=srt (p:L _>n;p.0) pairs;sk";
    run_all(src, "main", &[], "[[1, cherry], [2, banana], [3, apple]]");
}

#[test]
fn lambda_map_then_grp_no_tls_desync() {
    // grp is also tree-bridge for the 2-arg form (PR 3b). Confirm
    // the TLS desync didn't only affect srt. Key returns the parity
    // bucket as a string ("even" / "odd") so the grouped map keys
    // are predictable across engines.
    let src = "main>L n;a=map (x:n>n;*x 2) [1 2 3];g=grp (x:n>t;?>x 5 \"big\" \"small\") [1 6 2 7 3 8];mget g \"big\"";
    run_all(src, "main", &[], "[6, 7, 8]");
}

#[test]
fn lambda_map_then_uniqby_no_tls_desync() {
    // uniqby key: parity via `?` ternary (`%` isn't an ilo operator;
    // arithmetic mod isn't needed for this regression).
    let src = "main>L n;a=map (x:n>n;+x 1) [1 2 3];uniqby (x:n>b;>x 3) [1 2 3 4 5]";
    run_all(src, "main", &[], "[1, 4]");
}

#[test]
fn lambda_map_then_partition_no_tls_desync() {
    let src = "main>L (L n);a=map (x:n>n;+x 1) [1 2 3];partition (x:n>b;>x 2) [1 2 3 4 5]";
    run_all(src, "main", &[], "[[3, 4, 5], [1, 2]]");
}

#[test]
fn lambda_map_then_srt_then_map_no_tls_desync() {
    // Three HOFs back-to-back across the native/bridge boundary in
    // both directions: native → bridge → native. The post-bridge
    // native call still needs the program-aware FnRef resolution.
    let src = "main>L n;a=map (x:n>n;+x 1) [1 2 3];sk=srt (p:L _>n;p.0) [[2 \"a\"] [1 \"b\"]];map (x:n>n;*x 10) [4 5 6]";
    run_all(src, "main", &[], "[40, 50, 60]");
}
