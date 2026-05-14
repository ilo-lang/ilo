// Regression tests for Python-style negative indices on `slc`, `take`, `drop`.
//
// Background: PR #183 (2026-05-12) added negative-index support to `at xs i`.
// This change extends the same semantics to slice-shaped operators so the
// "negative index = count from the end" rule is uniform across every builtin
// that takes a position. Closes the quant-trader fencepost friction
// (assessment-feedback line 1374) and the `slc xs -np 1 np` ergonomics gap
// (line 753) by removing the workaround that bound `s=- np 1` before every
// last-element access.
//
// Coverage matrix: every engine (tree, VM, cranelift JIT) × every boundary
// case (`-len`, `-1`, `0`, `len`, beyond `len` both directions) × list and
// text. The three engines route through the shared `resolve_slice_bound` /
// `resolve_take_count` / `resolve_drop_count` helpers in `builtins.rs`, but
// the harness runs all three end-to-end to catch dispatch-layer regressions
// (e.g. the JIT helper returning `TAG_NIL` instead of unwinding into the
// shared helper).

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str) -> String {
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

fn check_all_engines(src: &str, entry: &str, expected: &str) {
    for engine in ["--run-tree", "--run-vm"] {
        assert_eq!(
            run(engine, src, entry),
            expected,
            "engine={engine} src=`{src}` entry={entry}"
        );
    }
    #[cfg(feature = "cranelift")]
    {
        assert_eq!(
            run("--run-cranelift", src, entry),
            expected,
            "engine=--run-cranelift src=`{src}` entry={entry}"
        );
    }
}

// ── slc: negative bounds on lists ─────────────────────────────────────────

// `slc xs -1 (len xs)` returns the last element as a 1-element list.
#[test]
fn slc_list_neg_start_to_len() {
    check_all_engines("f>L n;xs=[10,20,30,40,50];slc xs -1 5", "f", "[50]");
}

// `slc xs -2 (len xs)` returns the last two elements.
#[test]
fn slc_list_last_two() {
    check_all_engines("f>L n;xs=[10,20,30,40,50];slc xs -2 5", "f", "[40, 50]");
}

// `slc xs 0 -1` drops the last element.
#[test]
fn slc_list_neg_end_drops_last() {
    check_all_engines(
        "f>L n;xs=[10,20,30,40,50];slc xs 0 -1",
        "f",
        "[10, 20, 30, 40]",
    );
}

// `slc xs -3 -1` middle slice from negatives.
#[test]
fn slc_list_neg_both_bounds() {
    check_all_engines("f>L n;xs=[10,20,30,40,50];slc xs -3 -1", "f", "[30, 40]");
}

// `slc xs -len 0` is empty (start clamps to 0, end clamps to 0).
#[test]
fn slc_list_neg_len_to_zero_empty() {
    check_all_engines("f>L n;xs=[10,20,30,40,50];slc xs -5 0", "f", "[]");
}

// `slc xs -len (len xs)` is the full list.
#[test]
fn slc_list_neg_len_full() {
    check_all_engines(
        "f>L n;xs=[10,20,30,40,50];slc xs -5 5",
        "f",
        "[10, 20, 30, 40, 50]",
    );
}

// Indices beyond `-len` clamp to 0 — never out of range, never wraps.
#[test]
fn slc_list_neg_beyond_len_clamps_to_zero() {
    check_all_engines("f>L n;xs=[10,20,30,40,50];slc xs -99 -90", "f", "[]");
    check_all_engines(
        "f>L n;xs=[10,20,30,40,50];slc xs -99 3",
        "f",
        "[10, 20, 30]",
    );
}

// Positive end past len still clamps as before — pure regression for the
// existing happy path now that integer-validation is in place.
#[test]
fn slc_list_pos_end_past_len_clamps() {
    check_all_engines("f>L n;xs=[10,20,30];slc xs 0 99", "f", "[10, 20, 30]");
}

// Quant-trader fencepost: previously needed `npm=- np 1;at eq npm`. Now
// `slc eq -1 np` drops the last element cleanly.
#[test]
fn slc_quant_trader_fencepost() {
    check_all_engines(
        "f>L n;eq=[100,101,102,103,99];slc eq 0 -1",
        "f",
        "[100, 101, 102, 103]",
    );
}

// ── slc: negative bounds on text ──────────────────────────────────────────

#[test]
fn slc_text_neg_end_drops_last_char() {
    check_all_engines("f>t;slc \"hello\" 0 -1", "f", "hell");
}

#[test]
fn slc_text_last_three() {
    check_all_engines("f>t;slc \"hello\" -3 5", "f", "llo");
}

#[test]
fn slc_text_neg_beyond_len_clamps() {
    check_all_engines("f>t;slc \"hello\" -99 -1", "f", "hell");
}

// Multi-byte characters: negative slicing must operate on codepoints, not
// bytes. (The existing positive-slice path already does this; the helper
// inherits it because it works on the post-`chars().collect()` length.)
#[test]
fn slc_text_unicode_neg() {
    check_all_engines("f>t;slc \"héllo\" -3 5", "f", "llo");
}

// ── take: negative count drops tail ───────────────────────────────────────

#[test]
fn take_list_neg_one_drops_last() {
    check_all_engines(
        "f>L n;xs=[10,20,30,40,50];take -1 xs",
        "f",
        "[10, 20, 30, 40]",
    );
}

#[test]
fn take_list_neg_keeps_only_prefix() {
    check_all_engines("f>L n;xs=[10,20,30,40,50];take -2 xs", "f", "[10, 20, 30]");
}

#[test]
fn take_list_neg_len_empty() {
    check_all_engines("f>L n;xs=[10,20,30,40,50];take -5 xs", "f", "[]");
}

#[test]
fn take_list_neg_beyond_len_empty() {
    check_all_engines("f>L n;xs=[10,20,30,40,50];take -99 xs", "f", "[]");
}

#[test]
fn take_text_neg_drops_tail() {
    check_all_engines("f>t;take -2 \"hello\"", "f", "hel");
}

// ── drop: negative count keeps tail ───────────────────────────────────────

#[test]
fn drop_list_neg_one_keeps_last() {
    check_all_engines("f>L n;xs=[10,20,30,40,50];drop -1 xs", "f", "[50]");
}

#[test]
fn drop_list_neg_keeps_last_two() {
    check_all_engines("f>L n;xs=[10,20,30,40,50];drop -2 xs", "f", "[40, 50]");
}

#[test]
fn drop_list_neg_len_keeps_all() {
    check_all_engines(
        "f>L n;xs=[10,20,30,40,50];drop -5 xs",
        "f",
        "[10, 20, 30, 40, 50]",
    );
}

#[test]
fn drop_list_neg_beyond_len_keeps_all() {
    check_all_engines(
        "f>L n;xs=[10,20,30,40,50];drop -99 xs",
        "f",
        "[10, 20, 30, 40, 50]",
    );
}

#[test]
fn drop_text_neg_keeps_tail() {
    check_all_engines("f>t;drop -3 \"hello\"", "f", "llo");
}

// ── boundary: positive zero still works ──────────────────────────────────

// Make sure adding negative paths didn't break `take 0` / `drop 0` (empty
// / full respectively). These are the existing happy-path regressions.

#[test]
fn take_list_zero_empty() {
    check_all_engines("f>L n;xs=[10,20,30];take 0 xs", "f", "[]");
}

#[test]
fn drop_list_zero_full() {
    check_all_engines("f>L n;xs=[10,20,30];drop 0 xs", "f", "[10, 20, 30]");
}

// ── empty-list edge cases — every negative bound must be safe ────────────

#[test]
fn slc_empty_list_neg_bounds() {
    check_all_engines("f>L n;xs=[];slc xs -1 -1", "f", "[]");
    check_all_engines("f>L n;xs=[];slc xs -99 99", "f", "[]");
}

#[test]
fn take_empty_list_neg() {
    check_all_engines("f>L n;xs=[];take -3 xs", "f", "[]");
}

#[test]
fn drop_empty_list_neg() {
    check_all_engines("f>L n;xs=[];drop -3 xs", "f", "[]");
}
