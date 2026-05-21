// Cross-engine regression test pinning `srt`'s stability guarantee.
//
// Every backend uses Rust's stable `Vec::sort` / `sort_by` today:
//   - tree-walker: `src/interpreter/mod.rs` Builtin::Srt arms (1-arg, 2/3-arg).
//   - register VM: `OP_SRT` (1-arg fast + general path) and
//     `srt_by_key_finalize` (2-arg native lift), both `sort_by`.
//   - Cranelift JIT/AOT: `jit_srt` (1-arg) and `jit_srt_by_key` (2-arg)
//     reuse the same finalizer.
//
// log-timeline-merger persona flagged srt for "equal-timestamp ordering" —
// the implementation is already stable, but the docs never said so, so an
// agent had no way to know whether merging two log streams sorted by a
// shared `ts` would preserve per-source order inside each tie group. This
// test pins the contract so a future refactor to `sort_unstable_by` can't
// silently regress it.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn engines() -> Vec<&'static str> {
    let mut v = vec!["tree", "--vm"];
    if cfg!(feature = "cranelift") {
        v.push("--jit");
    }
    v
}

fn run_ok(engine: &str, src: &str, fn_name: &str) -> String {
    let out = match engine {
        "tree" => ilo()
            .args(["run", src, fn_name])
            .output()
            .expect("failed to run ilo"),
        _ => ilo()
            .args([src, engine, fn_name])
            .output()
            .expect("failed to run ilo"),
    };
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}` fn={fn_name}: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// `srt fn xs` with a key function that pulls from a captured parallel list.
// keys = [2, 1, 2, 1, 2], items = [0, 1, 2, 3, 4] (original indices).
// Stable sort puts the two key=1 entries (indices 1, 3) first, then the
// three key=2 entries (indices 0, 2, 4) — each group in input order.
// An unstable sort could legally return [3, 1, 4, 2, 0] or any other
// equal-key permutation.
#[test]
fn srt_by_key_is_stable_within_equal_keys() {
    let src = "\
f>L n
  keys=[2 1 2 1 2]
  srt (i:n>n;at keys i) [0 1 2 3 4]
";
    for e in engines() {
        let got = run_ok(e, src, "f");
        assert_eq!(got, "[1, 3, 0, 2, 4]", "engine={e}");
    }
}

// All-equal keys: the output must echo the input verbatim. An unstable
// sort could shuffle freely; stable sort is the identity here.
#[test]
fn srt_by_key_all_equal_preserves_input_order() {
    let src = "f>L n;srt (x:n>n;0) [4 3 2 1 0]";
    for e in engines() {
        let got = run_ok(e, src, "f");
        assert_eq!(got, "[4, 3, 2, 1, 0]", "engine={e}");
    }
}

// `srt xs` 1-arg form on numbers: with all-equal values the sort must
// preserve input order (trivially observable as an identity output).
// Numbers compare by IEEE-754 partial_cmp, so equal numbers compare Equal
// and stable sort keeps them in place.
#[test]
fn srt_one_arg_numbers_all_equal_is_identity() {
    let src = "f>L n;srt [5 5 5 5 5]";
    for e in engines() {
        let got = run_ok(e, src, "f");
        assert_eq!(got, "[5, 5, 5, 5, 5]", "engine={e}");
    }
}

// Log-timeline-merger shape: two streams of (ts, source-index) pairs
// already in per-source order, merged then sorted by ts. With stable
// `srt fn xs`, events that share a ts keep their input order — which
// after merging is "stream A first, stream B second" inside each tie.
// We model this by encoding pairs as indices into a parallel ts list.
#[test]
fn srt_by_key_merges_log_streams_preserving_per_source_order() {
    // Stream A: ts=10, 20, 30 (indices 0, 1, 2)
    // Stream B: ts=10, 20, 30 (indices 3, 4, 5)
    // Merged input: [0, 1, 2, 3, 4, 5] — A first, then B.
    // After stable sort by ts: pairs with ts=10 are [0, 3]; ts=20 are
    // [1, 4]; ts=30 are [2, 5]. Each pair preserves stream-A-first order.
    let src = "\
f>L n
  ts=[10 20 30 10 20 30]
  srt (i:n>n;at ts i) [0 1 2 3 4 5]
";
    for e in engines() {
        let got = run_ok(e, src, "f");
        assert_eq!(got, "[0, 3, 1, 4, 2, 5]", "engine={e}");
    }
}
