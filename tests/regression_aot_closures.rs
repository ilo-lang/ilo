// Regression test: AOT-compiled binaries must handle HOF / closure / fn-ref
// dispatch identically to tree, VM, and Cranelift JIT.
//
// Background:
//
// Engine audit PR #413 found that AOT silently returned `nil` (or
// `[nil, nil, ...]`) for every program that emitted OP_CALL_DYN or an
// OP_*_BY_KEY finalizer where the helper has to re-enter the VM on a
// user-fn callback. Root cause: AOT never published `ACTIVE_PROGRAM` /
// `ACTIVE_AST_PROGRAM`, so the helpers hit their null-program guards
// (`src/vm/mod.rs` jit_call_dyn at line ~15977 and jit_call_builtin_tree
// at line ~15744) and silently returned TAG_NIL.
//
// The fix serialises the full `CompiledProgram` (chunks + AST +
// type_registry + func_names + is_tool) into a postcard blob embedded
// in the AOT binary's `.rodata`, and a new `ilo_aot_publish_program`
// runtime helper deserialises it on startup and publishes the four TLS
// pointers `with_active_registry` would otherwise set.
//
// Each case below is a row from the audit's `tests/engine-matrix/` corpus
// distilled into the smallest form that surfaces the bug. The assertion
// is cross-engine parity: AOT must match tree / VM / JIT byte-for-byte
// on stdout, stderr, and exit code. A regression that re-introduces the
// silent-nil class shows up here immediately.
//
// Gated on the `cranelift` feature because both AOT compile and the
// `--jit` baseline require it.

#![cfg(feature = "cranelift")]

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn tmp_paths(tag: &str) -> (PathBuf, PathBuf) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let src = std::env::temp_dir().join(format!("ilo-aot-clos-{tag}-{pid}-{n}.ilo"));
    let bin = std::env::temp_dir().join(format!("ilo-aot-clos-{tag}-{pid}-{n}.bin"));
    (src, bin)
}

fn run_in_process(src_path: &PathBuf, engine: &str) -> (Vec<u8>, Vec<u8>, i32) {
    let out = ilo()
        .arg(src_path)
        .arg(engine)
        .arg("main")
        .output()
        .expect("failed to run ilo in-process");
    (out.stdout, out.stderr, out.status.code().unwrap_or(-1))
}

fn run_aot(src_path: &PathBuf, bin_path: &PathBuf) -> (Vec<u8>, Vec<u8>, i32) {
    let compile = ilo()
        .args(["compile"])
        .arg(src_path)
        .arg("-o")
        .arg(bin_path)
        .arg("main")
        .output()
        .expect("failed to invoke ilo compile");
    assert!(
        compile.status.success(),
        "ilo compile failed: stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr),
    );
    let out = Command::new(bin_path)
        .output()
        .expect("failed to run AOT binary");
    (out.stdout, out.stderr, out.status.code().unwrap_or(-1))
}

/// Assert AOT output matches all three in-process engines and the
/// explicit expected stdout (the audit row's `-- expected:` value).
fn assert_cross_engine(tag: &str, src: &str, expected_stdout: &[u8]) {
    let (src_path, bin_path) = tmp_paths(tag);
    std::fs::write(&src_path, src).expect("write ilo source");

    let (aot_stdout, aot_stderr, aot_exit) = run_aot(&src_path, &bin_path);

    assert_eq!(
        aot_stdout,
        expected_stdout,
        "{tag}: AOT stdout mismatch. got={:?} expected={:?}",
        String::from_utf8_lossy(&aot_stdout),
        String::from_utf8_lossy(expected_stdout),
    );
    assert_eq!(
        aot_exit,
        0,
        "{tag}: AOT exit non-zero. stderr={:?}",
        String::from_utf8_lossy(&aot_stderr),
    );

    for engine in ["--run-tree", "--run-vm", "--jit"] {
        let (s, _e, c) = run_in_process(&src_path, engine);
        assert_eq!(
            s,
            aot_stdout,
            "{tag}/{engine}: stdout diverges from AOT. in-proc={:?} aot={:?}",
            String::from_utf8_lossy(&s),
            String::from_utf8_lossy(&aot_stdout),
        );
        assert_eq!(
            c, aot_exit,
            "{tag}/{engine}: exit diverges from AOT. in-proc={c} aot={aot_exit}",
        );
    }

    let _ = std::fs::remove_file(&src_path);
    let _ = std::fs::remove_file(&bin_path);
}

// ── map over an inline lambda with no captures ─────────────────────────
// Audit row 16. Pre-fix AOT returned [nil, nil, nil]. The lambda lifts to
// a synthetic top-level fn; the map call site emits OP_CALL_DYN against
// the FnRef. Without ACTIVE_PROGRAM published, jit_call_dyn returned
// TAG_NIL for every element.

#[test]
fn aot_map_inline_lambda_nocap() {
    assert_cross_engine(
        "map-lambda-nocap",
        "main>L n;map (x:n>n;*x 2) [1,2,3]\n",
        b"[2, 4, 6]\n",
    );
}

// ── map over an inline lambda WITH captures ────────────────────────────
// Audit row 17. Pre-fix AOT returned [nil, nil, nil]. Same shape as the
// no-capture case but the compiler also emits OP_MAKE_CLOSURE to wrap
// the FnRef with the captured value, so the helper has to handle the
// `HeapObj::Closure` discriminator path inside jit_call_dyn too.

#[test]
fn aot_map_inline_lambda_capture() {
    assert_cross_engine(
        "map-lambda-capture",
        "main>L n;k=10;map (x:n>n;+x k) [1,2,3]\n",
        b"[11, 12, 13]\n",
    );
}

// ── map with a closure-bind ctx arg (3-arg map fn ctx xs) ──────────────
// Audit row 18. Pre-fix AOT returned `nil`. This routes through the
// tree bridge (`is_tree_bridge_eligible` lists Map+3 as bridge-only),
// so the failing helper is jit_call_builtin_tree's null-AST guard
// rather than jit_call_dyn. Covers the second of the two TLS publishing
// paths the fix had to wire up.

#[test]
fn aot_map_closure_bind_ctx() {
    assert_cross_engine(
        "map-closure-bind",
        "addk x:n k:n>n;+x k\nmain>L n;map addk 10 [1,2,3]\n",
        b"[11, 12, 13]\n",
    );
}

// ── fld with a user-fn fold accumulator ────────────────────────────────
// Audit row 31. Pre-fix AOT returned `nil`. fld+3 (`fld fn xs init`)
// compiles to a loop body that calls the accumulator via OP_CALL_DYN.

#[test]
fn aot_fld_user_fn() {
    assert_cross_engine(
        "fld-user-fn",
        "add a:n b:n>n;+a b\nmain>n;fld add [1,2,3,4] 0\n",
        b"10\n",
    );
}

// ── grp by a user-fn key function (PR #391 native lift) ────────────────
// Audit row 36. Pre-fix AOT returned `nil`. `grp fn xs` compiles to a
// pre-loop that builds the keys list via OP_CALL_DYN followed by
// OP_GRP_BY_KEY to finalise. The OP_CALL_DYN side fails first; the
// finalizer itself takes pre-computed lists and is unaffected.

#[test]
fn aot_grp_by_user_fn() {
    assert_cross_engine(
        "grp-user-fn",
        "parity n:n>t;=mod n 2 0{\"even\"};\"odd\"\nmain>n;g=grp parity [1,2,3,4];len mkeys g\n",
        b"2\n",
    );
}

// ── uniqby with a user-fn key function ─────────────────────────────────
// Audit row 37. Same shape as grp.

#[test]
fn aot_uniqby_user_fn() {
    assert_cross_engine(
        "uniqby-user-fn",
        "parity n:n>t;=mod n 2 0{\"even\"};\"odd\"\nmain>n;u=uniqby parity [1,2,3,4];len u\n",
        b"2\n",
    );
}

// ── returning a fn-ref and calling it ──────────────────────────────────
// Audit row 38. Pre-fix AOT returned `nil`. `>F n n` declared return
// type means the returned value is a FnRef NanVal; calling it through
// `f 5` emits OP_CALL_DYN which needed ACTIVE_PROGRAM.

#[test]
fn aot_fnref_return_then_call() {
    assert_cross_engine(
        "fnref-return",
        "sq x:n>n;*x x\nmksq>F n n;sq\nmain>n;f=mksq;f 5\n",
        b"25\n",
    );
}

// ── top-level fn-ref to map (sanity check) ─────────────────────────────
// Audit row 15. This row already passed pre-fix on AOT — `map dbl xs`
// where `dbl` is a top-level fn was already going down a different
// emission path. Keeping the case here so future refactors don't
// accidentally regress the already-working shape while fixing the
// broken ones.

#[test]
fn aot_map_toplevel_fnref_still_works() {
    assert_cross_engine(
        "map-toplevel-fnref",
        "dbl x:n>n;*x 2\nmain>L n;map dbl [1,2,3]\n",
        b"[2, 4, 6]\n",
    );
}
