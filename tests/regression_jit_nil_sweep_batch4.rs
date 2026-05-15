// Regression tests for the Cranelift JIT-helper permissive-nil sweep, batch 4.
//
// Helpers in scope (Group B — text + len/coerce helpers, fmt/format):
//   jit_fmt2, jit_trm, jit_upr, jit_lwr, jit_cap, jit_padl, jit_padr,
//   jit_ord, jit_chr, jit_chars, jit_unq, jit_frq.
//
// (jit_len, jit_str, jit_num were already routed through the
// JIT_RUNTIME_ERROR TLS cell in batch 3 — they are intentionally not in
// scope here.)
//
// Before this PR these helpers silently returned TAG_NIL on type-error or
// empty/invalid input where tree/VM correctly raise an "ILO-R009" runtime
// error. The fix threads a packed source-span immediate into each helper
// and routes the failure paths through `jit_set_runtime_error_with_span`
// (the TLS primitive from #254) so diagnostics render with a caret matching
// tree/VM.
//
// Most error-path coverage lives as unit tests in `src/vm/mod.rs` that
// drive the helpers directly — the ilo surface verifier rejects programs
// that statically mix types (ILO-T009 / ILO-T010 / ILO-T012), so error
// paths are not easily reachable from a single CLI program. The tests
// here focus on cross-engine happy-path parity, pinning that wiring the
// span/error threads did not regress the success cases across tree, VM,
// and Cranelift JIT.

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

fn check_all(src: &str, expected: &str) {
    check_stdout("--run-tree", src, expected);
    check_stdout("--run-vm", src, expected);
    #[cfg(feature = "cranelift")]
    check_stdout("--run-cranelift", src, expected);
}

// ── fmt2 ──────────────────────────────────────────────────────────────────

#[test]
fn fmt2_basic_cross_engine() {
    check_all("f>t;fmt2 3.14159 2", "3.14");
}

#[test]
fn fmt2_zero_digits_cross_engine() {
    check_all("f>t;fmt2 7 0", "7");
}

// ── trm / upr / lwr / cap ─────────────────────────────────────────────────

#[test]
fn trm_string_cross_engine() {
    check_all("f>t;trm \"  hi  \"", "hi");
}

#[test]
fn upr_string_cross_engine() {
    check_all("f>t;upr \"hi\"", "HI");
}

#[test]
fn lwr_string_cross_engine() {
    check_all("f>t;lwr \"HI\"", "hi");
}

#[test]
fn cap_string_cross_engine() {
    check_all("f>t;cap \"hello\"", "Hello");
}

// ── padl / padr ───────────────────────────────────────────────────────────

// Note: check_stdout / check_all trim() stdout, so leading/trailing pad
// space is squashed. We assert via a wrapper that includes a sentinel
// character on the inside, so the pad sits between the sentinel and the
// content where trim() leaves it alone.
fn check_all_no_trim(src: &str, expected: &str) {
    for engine in [
        "--run-tree",
        "--run-vm",
        #[cfg(feature = "cranelift")]
        "--run-cranelift",
    ] {
        let out = ilo()
            .args([src, engine, "f"])
            .output()
            .expect("failed to run ilo");
        assert!(
            out.status.success(),
            "engine={engine}: expected success for `{src}`, got stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        // Strip the trailing newline that println! / ilo emits but keep
        // any leading or interior whitespace intact.
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stdout = stdout.strip_suffix('\n').unwrap_or(&stdout);
        let stdout = stdout.strip_suffix('\r').unwrap_or(stdout);
        assert_eq!(
            stdout, expected,
            "engine={engine}: stdout mismatch for `{src}`"
        );
    }
}

#[test]
fn padl_string_cross_engine() {
    check_all_no_trim("f>t;padl \"hi\" 5", "   hi");
}

#[test]
fn padr_string_cross_engine() {
    check_all_no_trim("f>t;padr \"hi\" 5", "hi   ");
}

// ── ord / chr / chars ─────────────────────────────────────────────────────

#[test]
fn ord_string_cross_engine() {
    check_all("f>n;ord \"A\"", "65");
}

#[test]
fn chr_number_cross_engine() {
    check_all("f>t;chr 65", "A");
}

#[test]
fn chars_string_cross_engine() {
    check_all("f>L t;chars \"abc\"", "[a, b, c]");
}

// ── unq / frq ─────────────────────────────────────────────────────────────

#[test]
fn unq_string_cross_engine() {
    check_all("f>t;unq \"aabbc\"", "abc");
}

#[test]
fn unq_list_cross_engine() {
    check_all("f>L n;unq [1 2 2 3 1]", "[1, 2, 3]");
}

// ── No stale-error leak across successive Cranelift calls ─────────────────
//
// Mirrors the carrier test in batch 3: a helper-set error in an errored
// Cranelift call must not leak into the next fresh invocation. We can't
// easily provoke a batch-4 helper-driven error from surface ilo (verifier
// rejects mixed-type ops), so we use the empty-list `hd` path from batch
// 1 as the error carrier and a chars-on-a-string happy path afterwards.

#[test]
#[cfg(feature = "cranelift")]
fn no_stale_jit_error_leak_after_hd_error_then_chars() {
    let first = ilo()
        .args(["f>n;hd []", "--run-cranelift", "f"])
        .output()
        .expect("failed to run ilo");
    assert!(!first.status.success(), "first call should error on hd []");
    check_stdout("--run-cranelift", "f>L t;chars \"hi\"", "[h, i]");
}
