// Cross-engine regression coverage for Phase 2 inline lambdas (closure
// capture) on the VM. Phase 1 (PR #247) lifts `(params>ret;body)` to
// synthetic `__lit_N` decls; Phase 2 (PR #265) adds by-value capture of
// free variables and produces `Expr::MakeClosure` at the call site.
//
// Before this PR (#373 baseline) the VM compiler bailed at the
// MakeClosure compile site with `CompileError::UnsupportedClosureCapture`
// and the runtime silently fell back to the tree interpreter. With
// `OP_MAKE_CLOSURE` + closure-aware `OP_CALL_DYN` dispatch in place,
// every Phase 2 shape should run identically on tree and VM.
//
// Cranelift parity arrives in the next PR (PR2). For now the JIT path
// bails on chunks that contain OP_MAKE_CLOSURE (unknown opcode → return
// false in compile_function_body), which transparently falls back to
// the VM. So we run tree + VM here; PR2 expands the matrix.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(name: &str, src: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!("ilo_p2cap_{name}_{}_{n}.ilo", std::process::id()));
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

/// Run a Phase 2 inline-lambda program on every engine that supports
/// closure capture today (tree + VM). PR2 will widen this to include
/// `--run-cranelift` once the jit_call_dyn helper is closure-aware.
fn run_all(src: &str, entry: &str, args: &[&str], expected: &str) {
    for engine in ["--run-tree", "--run-vm"] {
        let actual = run_ok(engine, src, entry, args);
        assert_eq!(
            actual, expected,
            "engine {engine} produced {actual:?}, expected {expected:?} for src `{src}`"
        );
    }
}

// ── single-value capture ────────────────────────────────────────────────

#[test]
fn closure_single_capture_filter() {
    let src = "f xs:L n thr:n>L n;flt (x:n>b;>x thr) xs";
    run_all(src, "f", &["[1,5,3,8,2]", "4"], "[5, 8]");
}

#[test]
fn closure_single_capture_map() {
    let src = "f xs:L n bump:n>L n;map (x:n>n;+x bump) xs";
    run_all(src, "f", &["[1,2,3]", "10"], "[11, 12, 13]");
}

// ── multiple captures ───────────────────────────────────────────────────

#[test]
fn closure_two_captures_range_filter() {
    // Two captures appended in order: lo first, hi second.
    let src = "f xs:L n lo:n hi:n>L n;flt (x:n>b;&(>=x lo) <=x hi) xs";
    run_all(src, "f", &["[1,3,5,7,9,11]", "3", "7"], "[3, 5, 7]");
}

#[test]
fn closure_three_captures_in_map() {
    // Three captures: each appears in the body. Order matters because
    // the lifter places them as trailing params in source order.
    let src = "f xs:L n a:n b:n c:n>L n;map (x:n>n;+*x a +*x b c) xs";
    run_all(src, "f", &["[1,2,3]", "10", "100", "7"], "[117, 227, 337]");
}

// ── text capture ────────────────────────────────────────────────────────

#[test]
fn closure_capture_text_prefix() {
    // Capture a heap-allocated text value. Exercises the RC bump path
    // for non-numeric captures.
    let src = "f ws:L t prefix:t>L t;flt (w:t>b;has w prefix) ws";
    run_all(
        src,
        "f",
        &["[\"apple\",\"banana\",\"apricot\"]", "ap"],
        "[apple, apricot]",
    );
}

// ── by-value snapshot semantics ─────────────────────────────────────────

#[test]
fn closure_capture_is_snapshot_not_live() {
    // The closure must hold the capture's value at construction time,
    // independent of any later rebinding of the source register. We
    // emulate that by passing the bias straight through: srt has
    // already drained the closure by the time the function returns,
    // and the result must reflect the original bias.
    let src = "f xs:L n bias:n>L n;ys=srt (x:n>n;+x bias) xs;ys";
    run_all(src, "f", &["[3,1,2]", "0"], "[1, 2, 3]");
}

// ── capture in fold reducer ─────────────────────────────────────────────

#[test]
fn closure_capture_in_fld() {
    // The reducer takes two ordinary params (acc, x) and one capture
    // (weight) appended at the end.
    let src = "f xs:L n weight:n>n;fld (a:n x:n>n;+a *x weight) xs 0";
    run_all(src, "f", &["[1,2,3,4]", "5"], "50");
}

// ── capture order is preserved ──────────────────────────────────────────

#[test]
fn closure_capture_order_a_then_b() {
    // Captures `a` and `b` in that order; the body subtracts to verify
    // we didn't transpose them.
    let src = "f xs:L n a:n b:n>L n;map (x:n>n;+x -a b) xs";
    run_all(src, "f", &["[10,20]", "100", "30"], "[80, 90]");
}

#[test]
fn closure_capture_order_b_then_a() {
    // Same lambda body but the captures appear in the opposite order
    // in the enclosing fn's signature; the lifter should pick them up
    // in body-reference order, so the result is identical to the above.
    let src = "f xs:L n b:n a:n>L n;map (x:n>n;+x -a b) xs";
    run_all(src, "f", &["[10,20]", "30", "100"], "[80, 90]");
}

// ── ctx-bind 3-arg form alongside captures ──────────────────────────────

#[test]
fn closure_capture_with_explicit_ctx_arg_form() {
    // Phase 1 ctx-bind form keeps working when the lambda has no
    // captures — pins that the closure plumbing doesn't regress the
    // existing tree-bridge path.
    let src = "f xs:L n thr:n>L n;flt (x:n c:n>b;>x c) thr xs";
    run_all(src, "f", &["[1,5,3,8,2]", "4"], "[5, 8]");
}

// ── nested HOF call: capture flows through outer + inner lambda ────────

#[test]
fn closure_capture_in_sort_key_distance() {
    // srt with an inline key that closes over `target`. Two distinct
    // call sites per element: srt drives many comparisons internally,
    // so this exercises capture re-use across repeated invocations.
    let src = "f xs:L n target:n>L n;srt (x:n>n;abs -x target) xs";
    run_all(src, "f", &["[1,5,10,20]", "8"], "[10, 5, 1, 20]");
}

// ── capture with empty list edge case ───────────────────────────────────

#[test]
fn closure_capture_on_empty_list() {
    // Empty input list: closure is constructed but never invoked. The
    // capture's RC still needs to round-trip cleanly through OP_RET.
    let src = "f xs:L n thr:n>L n;flt (x:n>b;>x thr) xs";
    run_all(src, "f", &["[]", "4"], "[]");
}
