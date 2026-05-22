// Regression tests for `\xNN` hex escape sequences in string literals
// (ILO-39 / 0.13.0 text-utility batch, lexer PR B).
//
// `\xNN` encodes a single byte in U+0000..=U+00FF from two hex digits.
// This closes tui-client ANSI escape pain (e.g. `"\x1b[31m"` instead of the
// error-prone raw-escape approach).

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

// ── basic correctness ─────────────────────────────────────────────────────────

#[test]
fn hex_escape_null_byte() {
    // \x00 == \0 (NUL).
    let src = r#"f>b;= "\x00" "\0""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "true", "engine={e}");
    }
}

#[test]
fn hex_escape_lowercase_a() {
    // \x61 == "a".
    let src = r#"f>b;= "\x61" "a""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "true", "engine={e}");
    }
}

#[test]
fn hex_escape_uppercase_hex_digits() {
    // \x41 == "A".  Hex digits are case-insensitive.
    let src = r#"f>b;= "\x41" "A""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "true", "engine={e}");
    }
}

#[test]
fn hex_escape_mixed_case_hex_digits() {
    // \xAb == \xab == chr(171) == «.
    let src = r#"f>b;= "\xAb" "\xab""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "true", "engine={e}");
    }
}

#[test]
fn hex_escape_ansi_escape_char() {
    // \x1b == ESC (0x1B). The canonical motivation: ANSI colour codes.
    // We test the length (1 char) rather than printing the ESC byte.
    let src = r#"f>n;len "\x1b""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "1", "engine={e}");
    }
}

#[test]
fn hex_escape_inside_longer_string() {
    // Surrounding characters are preserved correctly.
    // "\x48ello" == "Hello"
    let src = r#"f>b;= "\x48ello" "Hello""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "true", "engine={e}");
    }
}

#[test]
fn hex_escape_multiple_in_one_string() {
    // \x68\x69 == "hi"
    let src = r#"f>b;= "\x68\x69" "hi""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "true", "engine={e}");
    }
}

// ── high byte (U+0080..=U+00FF) ───────────────────────────────────────────────

#[test]
fn hex_escape_high_byte_roundtrip() {
    // \xc3 encodes U+00C3 (Ã) and \xa9 encodes U+00A9 (©). Each is a 2-byte
    // UTF-8 sequence, so the total byte-length of the string is 4. We
    // round-trip via `= "\xc3\xa9" "Ã©"` to confirm the char values are
    // correct without relying on len byte/char semantics.
    let src = r#"f>b;= "\xc3\xa9" "Ã©""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "true", "engine={e}");
    }
}

// ── lenient pass-through on malformed sequences ───────────────────────────────

#[test]
fn hex_escape_non_hex_passed_through() {
    // \xZZ — non-hex digits → pass through as literal \xZZ (4 chars).
    let src = r#"f>n;len "\xZZ""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "4", "engine={e}");
    }
}
