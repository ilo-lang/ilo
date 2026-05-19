// Regression: Cranelift JIT used to bail out (`JitCallError::NotEligible`)
// on any function containing `OP_WINDOW` or `OP_WINDOW_VIEW` because the
// VM dispatcher emits `HeapObj::ListView` strides and Cranelift's inlined
// LISTGET / FOREACHPREP / FOREACHNEXT fast paths read Vec metadata at
// fixed offsets that would UB on a view's struct layout.
//
// Fix: route OP_WINDOW / OP_WINDOW_VIEW through their existing extern
// helpers (`jit_window` / `jit_window_view`) which always return owning
// `HeapObj::List`s. The function compiles, the rest of the hot path
// (e.g. bio's `flt all-h (window k seqs)`) runs JIT-native.
//
// These tests exercise the shapes that would previously have bailed, on
// every engine, asserting parity.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn engines() -> &'static [&'static str] {
    &["--vm", "--jit"]
}

fn run_ok(engine: &str, src: &str, fn_name: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine).arg(fn_name);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn window_compiles_in_cranelift() {
    // Plain OP_WINDOW: was the primary bail trigger.
    let src = "f xs:L n>L (L n);window 3 xs";
    for engine in engines() {
        assert_eq!(
            run_ok(engine, src, "f", &["1,2,3,4,5"]),
            "[[1, 2, 3], [2, 3, 4], [3, 4, 5]]",
            "engine={engine}"
        );
    }
}

#[test]
fn window_then_foreach_in_cranelift() {
    // Outer foreach over window result exercises the inlined FOREACHPREP
    // / FOREACHNEXT path that previously bailed when window was present.
    let src = "f xs:L n>n;s=0;@w (window 2 xs){s=+s hd w};s";
    for engine in engines() {
        assert_eq!(
            run_ok(engine, src, "f", &["10,20,30,40"]),
            // hd of each of [[10,20],[20,30],[30,40]] = 10+20+30 = 60
            "60",
            "engine={engine}"
        );
    }
}

#[test]
fn fused_flt_window_in_cranelift() {
    // Bio-canonical-shape: `flt p (window k xs)`. Emitter uses the fused
    // OP_WINDOW_VIEW two-word encoding. Without the fix, cranelift bails
    // on the whole function. With the fix, it compiles end-to-end.
    //
    // Predicate: window contains a 2 anywhere. We assert the count of
    // matching windows so the test is robust to formatting.
    let src = "headgt w:L n>b;>(hd w) 1\nf xs:L n>n;len (flt headgt (window 2 xs))";
    for engine in engines() {
        assert_eq!(
            run_ok(engine, src, "f", &["1,2,3,2,4"]),
            // pairs: [1,2] [2,3] [3,2] [2,4] — heads 1,2,3,2 — > 1 → 3 matches
            "3",
            "engine={engine}"
        );
    }
}

#[test]
fn window_size_one_in_cranelift() {
    let src = "f xs:L n>L (L n);window 1 xs";
    for engine in engines() {
        assert_eq!(
            run_ok(engine, src, "f", &["7,8,9"]),
            "[[7], [8], [9]]",
            "engine={engine}"
        );
    }
}

#[test]
fn window_n_greater_than_len_in_cranelift() {
    let src = "f xs:L n>L (L n);window 5 xs";
    for engine in engines() {
        assert_eq!(
            run_ok(engine, src, "f", &["1,2,3"]),
            "[]",
            "engine={engine}"
        );
    }
}
