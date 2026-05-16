//! Cross-engine regression tests for the fused `flt fn (window n xs)` /
//! `map fn (window n xs)` emitter path.
//!
//! Background: bioinformatics rerun5 surfaced a 5.8x slowdown on
//! `flt all-hydro (window 15 (chars seq))` over an 11.4M-residue corpus.
//! Root cause was the unfused emitter materialising a `L (L t)` of
//! `n-k+1` small inner lists, each a fresh `Vec`. The fused emitter walks
//! `xs` once with stride 1, reusing a single scratch list as the per-call
//! window. The VM dispatcher's `OP_WINDOW_VIEW` arm (and the Cranelift
//! JIT/AOT helper `jit_window_view`) handle the in-place reuse via the
//! same RC-peek pattern used by `OP_ADD_SS` / `OP_LISTAPPEND` / `OP_MSET`.
//!
//! These tests pin:
//!   1. Output parity across tree/VM/Cranelift for `flt` and `map` over
//!      `window` (including empty / short / pass-through edge cases).
//!   2. A bool-typecheck error path: predicates that return a non-bool
//!      raise `flt: predicate must return bool` on every engine.
//!   3. A negative-fusion case: `xs = window n xs` (window result escapes
//!      to a binding rather than being consumed by `flt` / `map`) keeps
//!      the eager materialisation path and produces correct output.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

const ENGINES: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];

fn run(engine: &str, src: &str, entry: &str) -> std::process::Output {
    ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo")
}

fn run_ok(engine: &str, src: &str, entry: &str) -> String {
    let out = run(engine, src, entry);
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn fused_flt_window_keeps_matching_windows() {
    let src = "p w:L n>b;>(sum w) 5\nf>L (L n);flt p (window 2 [1,2,3,4])";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "f"), "[[3, 4]]", "engine={engine}");
    }
}

#[test]
fn fused_flt_window_all_pass_keeps_all() {
    // All windows pass — exercises the path where every iteration's
    // OP_LISTAPPEND clone_rc's the window scratch into the accumulator,
    // forcing the next OP_WINDOW_VIEW to allocate a fresh list because
    // strong count > 1. Cross-engine identical output proves the
    // ownership-transfer is correct.
    let src = "p w:L n>b;>(len w) 0\nf>L (L n);flt p (window 2 [10,20,30])";
    for engine in ENGINES {
        assert_eq!(
            run_ok(engine, src, "f"),
            "[[10, 20], [20, 30]]",
            "engine={engine}"
        );
    }
}

#[test]
fn fused_flt_window_empty_input() {
    let src = "p w:L n>b;>(sum w) 0\nf>n;len (flt p (window 3 []))";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "f"), "0", "engine={engine}");
    }
}

#[test]
fn fused_flt_window_size_exceeds_input() {
    // n > len(xs) — limit register goes <= 0, loop body never runs.
    let src = "p w:L n>b;>(sum w) 0\nf>L (L n);flt p (window 10 [1,2,3])";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "f"), "[]", "engine={engine}");
    }
}

#[test]
fn fused_map_window_sums_matches_tree() {
    let src = "f>L n;map sum (window 3 [1,2,3,4,5])";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "f"), "[6, 9, 12]", "engine={engine}");
    }
}

#[test]
fn fused_map_window_empty_input() {
    let src = "f>L n;map sum (window 3 [])";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "f"), "[]", "engine={engine}");
    }
}

#[test]
fn fused_map_window_size_exceeds_input() {
    let src = "f>L n;map sum (window 10 [1,2,3])";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "f"), "[]", "engine={engine}");
    }
}

#[test]
fn fused_flt_window_non_bool_predicate_errors() {
    // Predicate returns a number — must error with the same "flt:
    // predicate must return bool" message on every engine. Pins the
    // typecheck path inside the fused emitter (which mirrors the unfused
    // `(Builtin::Flt, 2)` arm byte-for-byte).
    let src = "p w:L n>n;sum w\nf>L (L n);flt p (window 2 [1,2,3])";
    for engine in ENGINES {
        let out = run(engine, src, "f");
        assert!(
            !out.status.success(),
            "ilo {engine} unexpectedly succeeded; expected non-bool predicate error"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("bool"),
            "engine={engine}: expected 'bool' in stderr, got: {stderr}"
        );
    }
}

#[test]
fn unfused_window_escape_path_still_works() {
    // `window` result bound to a variable rather than being consumed by
    // an outer `flt` / `map` — the fused emitter must NOT fire, falling
    // through to the eager OP_WINDOW dispatch. The output is the full
    // list of windows, which is what every engine has always produced.
    let src = "f>L (L n);ws=window 3 [10,20,30,40];ws";
    for engine in ENGINES {
        assert_eq!(
            run_ok(engine, src, "f"),
            "[[10, 20, 30], [20, 30, 40]]",
            "engine={engine}"
        );
    }
}

#[test]
fn fused_flt_window_size_one() {
    // Size-1 windows: each window is a single-element list. Exercises the
    // smallest non-trivial window. The reused scratch list keeps capacity
    // 1 across iterations.
    let src = "p w:L n>b;>(hd w) 1\nf>L (L n);flt p (window 1 [0,2,3])";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "f"), "[[2], [3]]", "engine={engine}");
    }
}

#[test]
fn fused_map_window_size_one_to_doubled() {
    // map (w >  hd w * 2) — exercises the path where the mapper consumes
    // the window by value and the scratch list's RC stays at 1 across the
    // entire walk (one allocation total).
    let src = "f w:L n>n;*(hd w) 2\nm>L n;map f (window 1 [3,5,7])";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "m"), "[6, 10, 14]", "engine={engine}");
    }
}
