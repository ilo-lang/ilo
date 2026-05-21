// Cross-engine regression tests for the URL + base64url encoding cluster:
//   - urlenc s > t            (RFC 3986 percent-encode)
//   - urldec s > R t t        (inverse, Err on malformed)
//   - b64u    s > t           (RFC 4648 §5 base64url, no padding)
//   - b64u-dec s > R t t      (inverse, Err on malformed)
//
// All four are tree-bridge eligible: VM and Cranelift dispatch through the
// tree interpreter, so all engines share the same semantics. Tests fan
// across available engines to catch any future divergence.

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

// ── urlenc: happy-path ───────────────────────────────────────────────────────

#[test]
fn urlenc_simple_ascii_unchanged() {
    // Unreserved chars (ALPHA / DIGIT / -._~) pass through literally.
    let src = "f>t;urlenc \"abc-123_XYZ.~\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "abc-123_XYZ.~", "engine={e}");
    }
}

#[test]
fn urlenc_reserved_chars_encoded() {
    // Space, `&`, `=` — the canonical OAuth/query-string trap.
    let src = "f>t;urlenc \"a b&c=d\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "a%20b%26c%3Dd", "engine={e}");
    }
}

#[test]
fn urlenc_utf8_multibyte() {
    // UTF-8: each byte percent-encoded individually. café = 63 61 66 c3 a9.
    let src = "f>t;urlenc \"café\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "caf%C3%A9", "engine={e}");
    }
}

// ── urldec: happy-path + error cases ─────────────────────────────────────────

#[test]
fn urldec_round_trip_through_urlenc() {
    // Composition: urldec(urlenc(s)) == s for any valid UTF-8 text.
    let src = "f>R t t;urldec (urlenc \"hello world & friends=42\")";
    for e in ENGINES {
        assert_eq!(
            run_ok(e, src, "f"),
            "hello world & friends=42",
            "engine={e}"
        );
    }
}

#[test]
fn urldec_recovers_utf8_multibyte() {
    let src = "f>R t t;urldec \"caf%C3%A9\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "café", "engine={e}");
    }
}

#[test]
fn urldec_invalid_escape_errors() {
    // Stray `%` without two hex digits — must produce Err, not silently pass.
    let src = "f>t;r=urldec \"abc%\";?r{~_:\"ok\";^_:\"err\"}";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "err", "engine={e}");
    }
}

#[test]
fn urldec_short_escape_errors() {
    // Only one hex digit after `%`.
    let src = "f>t;r=urldec \"abc%2\";?r{~_:\"ok\";^_:\"err\"}";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "err", "engine={e}");
    }
}

// ── b64u: happy-path ─────────────────────────────────────────────────────────

#[test]
fn b64u_simple_ascii() {
    // "hello" → "aGVsbG8" (no padding).
    let src = "f>t;b64u \"hello\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "aGVsbG8", "engine={e}");
    }
}

#[test]
fn b64u_uses_urlsafe_alphabet() {
    // Bytes that would standard-base64 to `+` / `/` must produce `-` / `_`.
    // 0xfb 0xff 0xbf → standard "+/+/" → base64url "-_-_".
    // Test via a string whose UTF-8 maps to a known url-unsafe encoding.
    // "??>" is bytes 3f 3f 3e → "Pz8+" in standard, "Pz8-" in url-safe.
    let src = "f>t;b64u \"??>\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "Pz8-", "engine={e}");
    }
}

#[test]
fn b64u_no_padding_emitted() {
    // 1 byte input → standard b64 "YQ==" → b64url "YQ" (padding stripped).
    let src = "f>t;b64u \"a\"";
    for e in ENGINES {
        let out = run_ok(e, src, "f");
        assert_eq!(out, "YQ", "engine={e}");
        assert!(!out.contains('='), "engine={e}: padding leaked: {out}");
    }
}

// ── b64u-dec: round-trip + error case ────────────────────────────────────────

#[test]
fn b64u_dec_round_trip() {
    // b64u-dec(b64u(s)) == s for any UTF-8 text.
    let src = "f>R t t;b64u-dec (b64u \"hello, world! 🎉\")";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "hello, world! 🎉", "engine={e}");
    }
}

#[test]
fn b64u_dec_recovers_known_value() {
    // "aGVsbG8" → "hello".
    let src = "f>R t t;b64u-dec \"aGVsbG8\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "hello", "engine={e}");
    }
}

#[test]
fn b64u_dec_invalid_alphabet_errors() {
    // `!` is not in the base64url alphabet → Err.
    let src = "f>t;r=b64u-dec \"abc!def\";?r{~_:\"ok\";^_:\"err\"}";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "err", "engine={e}");
    }
}

#[test]
fn b64u_dec_standard_padding_rejected() {
    // `=` padding is invalid in URL_SAFE_NO_PAD — strict round-trip contract.
    let src = "f>t;r=b64u-dec \"aGVsbG8=\";?r{~_:\"ok\";^_:\"err\"}";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "err", "engine={e}");
    }
}

#[test]
fn b64u_dec_invalid_utf8_errors() {
    // Bytes 0xff 0xff in base64url is "__8" (3 base64 chars → 2 bytes).
    // 0xff 0xff is not valid UTF-8 → b64u-dec must Err.
    let src = "f>t;r=b64u-dec \"__8\";?r{~_:\"ok\";^_:\"err\"}";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "err", "engine={e}");
    }
}

// ── Cross-builtin: realistic JWT-segment shape ───────────────────────────────

#[test]
fn b64u_jwt_header_shape() {
    // The classic JWT header {"alg":"HS256","typ":"JWT"} encodes to a known
    // url-safe value. This is the load-bearing use case for b64u.
    let src = "f>t;b64u \"{\\\"alg\\\":\\\"HS256\\\",\\\"typ\\\":\\\"JWT\\\"}\"";
    for e in ENGINES {
        assert_eq!(
            run_ok(e, src, "f"),
            "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9",
            "engine={e}"
        );
    }
}
