// Regression tests for the Cranelift JIT-helper permissive-nil sweep, batch 5.
//
// Helpers in scope (Group C — collections):
//   jit_spl, jit_cat, jit_has, jit_range,
//   jit_window, jit_zip, jit_chunks, jit_enumerate,
//   jit_setunion, jit_setinter, jit_setdiff,
//   jit_rev, jit_srt, jit_rsrt, jit_cumsum.
//
// Before this PR these helpers silently returned TAG_NIL on failure paths
// where tree/VM raise runtime errors. The fix routes the failure paths
// through the `JIT_RUNTIME_ERROR` TLS cell introduced in #254, threading a
// packed source-span immediate so diagnostics render with a caret matching
// tree/VM.
//
// As with batches 3/4: the ilo source-level verifier rejects programs that
// statically mix types (ILO-T009 / ILO-T010 / ILO-T012), so per-helper
// error-path tests live as unit tests inside `src/vm/mod.rs` that drive the
// helpers directly. These CLI tests focus on cross-engine happy-path parity,
// pinning that wiring the span/error threads did not regress the success
// cases (split, join, has, range, window, zip, chunks, enumerate, set ops,
// rev, srt/rsrt, cumsum) across tree, VM, and Cranelift JIT.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn check_stdout(engine: &str, src: &str, expected: &str) {
    let out = ilo()
        .args([src, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "engine={engine}: expected success for `{src}`, got stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        expected,
        "engine={engine}: stdout mismatch for `{src}`"
    );
}

// Run a check across all three engines and assert identical output.
fn check_all(src: &str, expected: &str) {
    check_stdout("--run-tree", src, expected);
    check_stdout("--run-vm", src, expected);
    #[cfg(feature = "cranelift")]
    check_stdout("--run-cranelift", src, expected);
}

// ── spl / cat happy paths ─────────────────────────────────────────────────

#[test]
fn spl_basic_cross_engine() {
    check_all("f>L t;spl \"a,b,c\" \",\"", "[a, b, c]");
}

#[test]
fn cat_basic_cross_engine() {
    check_all("f>t;cat [\"a\" \"b\" \"c\"] \",\"", "a,b,c");
}

// ── has happy paths ───────────────────────────────────────────────────────

#[test]
fn has_text_found_cross_engine() {
    check_all("f>b;has \"hello world\" \"world\"", "true");
}

#[test]
fn has_text_not_found_cross_engine() {
    check_all("f>b;has \"hello\" \"xyz\"", "false");
}

#[test]
fn has_list_found_cross_engine() {
    check_all("f>b;has [1 2 3] 2", "true");
}

#[test]
fn has_list_not_found_cross_engine() {
    check_all("f>b;has [1 2 3] 5", "false");
}

// ── range happy paths ─────────────────────────────────────────────────────

#[test]
fn range_basic_cross_engine() {
    check_all("f>L n;range 0 4", "[0, 1, 2, 3]");
}

#[test]
fn range_empty_when_start_ge_end_cross_engine() {
    check_all("f>L n;range 5 5", "[]");
}

// ── window / zip / chunks / enumerate happy paths ─────────────────────────

#[test]
fn window_basic_cross_engine() {
    check_all("f>L (L n);window 2 [1 2 3 4]", "[[1, 2], [2, 3], [3, 4]]");
}

#[test]
fn window_larger_than_input_cross_engine() {
    check_all("f>L (L n);window 5 [1 2 3]", "[]");
}

#[test]
fn zip_basic_cross_engine() {
    check_all(
        "f>L (L n);zip [1 2 3] [10 20 30]",
        "[[1, 10], [2, 20], [3, 30]]",
    );
}

#[test]
fn zip_truncates_to_shorter_cross_engine() {
    check_all("f>L (L n);zip [1 2 3 4] [10 20]", "[[1, 10], [2, 20]]");
}

#[test]
fn chunks_basic_cross_engine() {
    check_all("f>L (L n);chunks 2 [1 2 3 4 5]", "[[1, 2], [3, 4], [5]]");
}

#[test]
fn enumerate_basic_cross_engine() {
    check_all(
        "f>L (L n);enumerate [10 20 30]",
        "[[0, 10], [1, 20], [2, 30]]",
    );
}

// ── set ops happy paths ───────────────────────────────────────────────────

#[test]
fn setunion_basic_cross_engine() {
    check_all("f>L n;setunion [1 2 3] [3 4 5]", "[1, 2, 3, 4, 5]");
}

#[test]
fn setinter_basic_cross_engine() {
    check_all("f>L n;setinter [1 2 3 4] [3 4 5 6]", "[3, 4]");
}

#[test]
fn setdiff_basic_cross_engine() {
    check_all("f>L n;setdiff [1 2 3 4] [3 4 5]", "[1, 2]");
}

// ── rev / srt / rsrt happy paths ──────────────────────────────────────────

#[test]
fn rev_string_cross_engine() {
    check_all("f>t;rev \"hello\"", "olleh");
}

#[test]
fn rev_list_cross_engine() {
    check_all("f>L n;rev [1 2 3]", "[3, 2, 1]");
}

#[test]
fn srt_numbers_cross_engine() {
    check_all("f>L n;srt [3 1 2]", "[1, 2, 3]");
}

#[test]
fn srt_strings_cross_engine() {
    check_all("f>L t;srt [\"c\" \"a\" \"b\"]", "[a, b, c]");
}

#[test]
fn srt_empty_list_cross_engine() {
    check_all("f>L n;srt []", "[]");
}

#[test]
fn rsrt_numbers_cross_engine() {
    check_all("f>L n;rsrt [1 3 2]", "[3, 2, 1]");
}

#[test]
fn rsrt_strings_cross_engine() {
    check_all("f>L t;rsrt [\"a\" \"c\" \"b\"]", "[c, b, a]");
}

// ── cumsum happy paths ────────────────────────────────────────────────────

#[test]
fn cumsum_basic_cross_engine() {
    check_all("f>L n;cumsum [1 2 3 4]", "[1, 3, 6, 10]");
}

#[test]
fn cumsum_empty_cross_engine() {
    check_all("f>L n;cumsum []", "[]");
}
