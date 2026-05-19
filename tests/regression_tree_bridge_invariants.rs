// Tree-bridge invariants: pin the bridge-dispatch contract for every
// op that still routes through `is_tree_bridge_eligible` (src/vm/mod.rs:532)
// after `--run-tree` was removed from the public CLI surface in 0.12.1.
//
// The tree-walker is no longer user-selectable, but it stays in-tree as
// the runtime for HOF callbacks, regex, fmt variadic, IO, sleep, ct,
// rsrt, and the 3-arg ctx variants of map/flt/fld/srt. The VM bails to
// it transparently for those shapes; the JIT and AOT inherit the same
// behaviour because they share the VM's dispatch table.
//
// These tests pin VM ≡ JIT byte-for-byte across every bridge op. The
// tree-walker itself is exercised through the VM bridge path on the
// `--run-vm` invocation, so cross-engine parity here = cross-engine
// parity through the bridge. If a future PR3d/PR3e lifts any of these
// shapes natively, the tests must still pass — they pin the user-
// observable contract, not the dispatch internals.
//
// Why this file exists: when 0.13.0 hard-drops the tree-walker, a
// maintainer touching the dispatch table needs an unambiguous list of
// what user code expects from the bridge today. Adding this file with
// the corresponding entries in `is_tree_bridge_eligible` gives them a
// concrete fixture per op.

use std::process::Command;

const ENGINES: &[&str] = if cfg!(feature = "cranelift") {
    &["--run-vm", "--jit"]
} else {
    &["--run-vm"]
};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_engine(src: &str, engine: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
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
    check_entry(src, "f", expected);
}

fn check_entry(src: &str, entry: &str, expected: &str) {
    for engine in ENGINES {
        let actual = run_engine(src, engine, entry);
        assert_eq!(
            actual, expected,
            "engine={engine}, src=`{src}`: got `{actual}`, expected `{expected}`"
        );
    }
}

// ── rgxall1 (flat first-group convenience over rgxall) ─────────────────

#[test]
fn rgxall1_first_group_flat() {
    // rgxall1: 0 or 1 capture group only — flat L t of the first capture
    // group across every match. Pinned cross-engine to catch any
    // divergence between VM bridge dispatch and a future native lift.
    check(r#"f>L t;rgxall1 "(\w+)=\d+" "x=1 y=22 z=333""#, "[x, y, z]");
}

// ── rgxsub (regex substitution) ────────────────────────────────────────

#[test]
fn rgxsub_replaces_all_matches() {
    check(r#"f>t;rgxsub "\d+" "N" "a1 b22 c333""#, "aN bN cN");
}

#[test]
fn rgxsub_with_backref() {
    // Backref in replacement: $1 expands to first capture group.
    check(r#"f>t;rgxsub "(\w+)=(\d+)" "$1:$2" "x=1 y=22""#, "x:1 y:22");
}

// ── fmt2 (decimal precision formatter) ────────────────────────────────

#[test]
fn fmt2_decimal_precision() {
    // fmt2 x digits → text with `digits` fractional places (half-to-even).
    // Routes through the bridge in the 2-arg shape.
    check(r#"f>t;fmt2 3.14159 2"#, "3.14");
}

#[test]
fn fmt2_zero_digits_rounds_to_int() {
    // 0 digits = round-to-int (banker's rounding); 2.5 → 2.
    check(r#"f>t;fmt2 2.5 0"#, "2");
}

// ── sleep (IO: lossless round-trip through the bridge) ────────────────

#[test]
fn sleep_returns_nil_cross_engine() {
    // sleep 0 (no actual wait, but exercises the dispatch path). Returns
    // Nil; pinned identical across VM and JIT.
    check(r#"f>n;sleep 0;42"#, "42");
}

// ── ct: count by predicate (2-arg and 3-arg ctx) ───────────────────────

#[test]
fn ct_2arg_fnref_predicate() {
    // ct fn xs — count xs that satisfy the predicate. FnRef arg routes
    // through the tree-walker for user-fn dispatch.
    check(
        r#"pos x:n>b;>x 0
f>n;ct pos [-1,2,-3,4,5]"#,
        "3",
    );
}

#[test]
fn ct_3arg_ctx_predicate() {
    // ct fn ctx xs — closure-bind ctx variant. The ctx arg is the
    // closure-bind form the native emitters don't shape today.
    check(
        r#"gt x:n threshold:n>b;>x threshold
f>n;ct gt 2 [1,2,3,4,5]"#,
        "3",
    );
}

// ── rsrt: descending sort by key (2-arg and 3-arg ctx) ─────────────────

#[test]
fn rsrt_2arg_descending_sort() {
    check(
        r#"keyfn x:n>n;x
f>L n;rsrt keyfn [3,1,4,1,5,9,2,6]"#,
        "[9, 6, 5, 4, 3, 2, 1, 1]",
    );
}

#[test]
fn rsrt_3arg_ctx_sort() {
    check(
        r#"weighted x:n bias:n>n;+x bias
f>L n;rsrt weighted 100 [3,1,4,1,5]"#,
        "[5, 4, 3, 1, 1]",
    );
}

// ── map / flt / fld 3-arg ctx (closure-bind ctx variants) ──────────────

#[test]
fn map_3arg_ctx_closure_bind() {
    // map fn ctx xs — the ctx arg gets bound into the user fn callback.
    // Bridge routes because native emitters don't shape the extra arg.
    check(
        r#"addc x:n c:n>n;+x c
f>L n;map addc 10 [1,2,3]"#,
        "[11, 12, 13]",
    );
}

#[test]
fn flt_3arg_ctx_closure_bind() {
    check(
        r#"gt x:n threshold:n>b;>x threshold
f>L n;flt gt 2 [1,2,3,4,5]"#,
        "[3, 4, 5]",
    );
}

#[test]
fn fld_4arg_ctx_closure_bind() {
    // fld fn ctx xs init — accumulator with closure-bound ctx. The
    // 4-arg shape with a ctx is the canonical bridge consumer for fld.
    check(
        r#"addw acc:n x:n weight:n>n;+acc (*x weight)
f>n;fld addw 2 [1,2,3] 0"#,
        "12",
    );
}

#[test]
fn srt_3arg_ctx_closure_bind() {
    // srt fn ctx xs — closure-bind ascending sort by key.
    check(
        r#"weighted x:n bias:n>n;+x bias
f>L n;srt weighted 0 [3,1,4,1,5]"#,
        "[1, 1, 3, 4, 5]",
    );
}

// ── Composition: bridge ops chained through the VM ─────────────────────

#[test]
fn bridge_ops_chained_in_one_program() {
    // Run rgxall1 → flt(3-arg ctx) → ct(2-arg) in one program; every
    // hop goes through the bridge, exercising the bridge re-entry path
    // multiple times in a single VM invocation.
    check(
        r#"pos x:n>b;>x 0
f>n;ct pos (map (s:t>n;len s) (rgxall1 "(\w+)" "a bb ccc"))"#,
        "3",
    );
}
