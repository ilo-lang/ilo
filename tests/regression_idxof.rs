// Regression tests for `idxof s sub > O n` — first code-point index of `sub`
// in `s`, nil when absent (ILO-39 / 0.13.0 text-utility batch).
//
// All tests fan across available engines (VM; JIT when cranelift feature is on)
// since idxof is tree-bridge eligible and VM + Cranelift must agree.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

#[cfg(feature = "cranelift")]
const ENGINES: &[&str] = &["--vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES: &[&str] = &["--vm"];

fn run_ok(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} {src:?} {entry:?} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// ── happy-path ────────────────────────────────────────────────────────────────

#[test]
fn idxof_found_at_start() {
    let src = "f>O n;idxof \"hello\" \"hel\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "0", "engine={e}");
    }
}

#[test]
fn idxof_found_in_middle() {
    let src = "f>O n;idxof \"hello world\" \"world\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "6", "engine={e}");
    }
}

#[test]
fn idxof_found_at_end() {
    let src = "f>O n;idxof \"abcdef\" \"ef\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "4", "engine={e}");
    }
}

#[test]
fn idxof_single_char() {
    let src = "f>O n;idxof \"abcde\" \"c\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "2", "engine={e}");
    }
}

#[test]
fn idxof_not_found_returns_nil() {
    // Use nil-coalesce to convert nil → -1 for easy assertion.
    let src = "f>n;?? (idxof \"hello\" \"xyz\") -1";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "-1", "engine={e}");
    }
}

#[test]
fn idxof_empty_needle_returns_zero() {
    // Empty sub matches at position 0 (Python / JS convention).
    let src = "f>O n;idxof \"hello\" \"\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "0", "engine={e}");
    }
}

#[test]
fn idxof_empty_haystack_not_found() {
    let src = "f>n;?? (idxof \"\" \"a\") -1";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "-1", "engine={e}");
    }
}

#[test]
fn idxof_both_empty_returns_zero() {
    let src = "f>O n;idxof \"\" \"\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "0", "engine={e}");
    }
}

// ── Unicode: index is in code-point units, not bytes ─────────────────────────

#[test]
fn idxof_unicode_codepoint_index() {
    // "café" = c(0) a(1) f(2) é(3). "é" is a 2-byte UTF-8 sequence; the
    // code-point index should be 3, not the byte offset 4.
    let src = "f>O n;idxof \"café\" \"é\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "3", "engine={e}");
    }
}

#[test]
fn idxof_unicode_multibyte_substring() {
    // "日本語" — find "本語" which starts at code-point 1.
    let src = "f>O n;idxof \"日本語\" \"本語\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "1", "engine={e}");
    }
}

