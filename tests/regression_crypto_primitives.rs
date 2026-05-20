// Cross-engine regression tests for the crypto primitive builtins added in 0.12.1:
//   sha256, hmac-sha256, base64-enc, base64-dec, base64url-enc, base64url-dec,
//   hex-enc, hex-dec, ct-eq.
//
// All are tree-bridge eligible: VM and Cranelift dispatch through the tree
// interpreter so every engine shares the same semantics. Tests use known
// vectors (FIPS 180-4, RFC 4231, RFC 4648) to catch any future divergence.

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

fn run_err(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} {src:?} {entry:?} unexpectedly succeeded: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

// ── sha256 ───────────────────────────────────────────────────────────────────

#[test]
fn sha256_empty_string() {
    // FIPS 180-4 known vector: SHA-256("") =
    // e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
    let src = r#"f>t;sha256 """#;
    for e in ENGINES {
        assert_eq!(
            run_ok(e, src, "f"),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "engine={e}"
        );
    }
}

#[test]
fn sha256_abc() {
    // SHA-256("abc") verified with coreutils sha256sum
    let src = r#"f>t;sha256 "abc""#;
    for e in ENGINES {
        assert_eq!(
            run_ok(e, src, "f"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            "engine={e}"
        );
    }
}

#[test]
fn sha256_hello_world() {
    // SHA-256("hello world") verified with coreutils sha256sum
    let src = r#"f>t;sha256 "hello world""#;
    for e in ENGINES {
        assert_eq!(
            run_ok(e, src, "f"),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
            "engine={e}"
        );
    }
}

#[test]
fn sha256_produces_64_char_hex() {
    // SHA-256 output is always 32 bytes = 64 hex chars.
    let src = r#"f>n;len (sha256 "test")"#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "64", "engine={e}");
    }
}

// ── hmac-sha256 ──────────────────────────────────────────────────────────────

#[test]
fn hmac_sha256_rfc4231_ascii_key() {
    // RFC 4231 Test Case 2: key = "Jefe", data = "what do ya want for nothing?"
    // HMAC-SHA-256 = 5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843
    // Verified with: echo -n "what do ya want for nothing?" | openssl dgst -sha256 -hmac "Jefe"
    let src = r#"f>t;hmac-sha256 "Jefe" "what do ya want for nothing?""#;
    for e in ENGINES {
        assert_eq!(
            run_ok(e, src, "f"),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
            "engine={e}"
        );
    }
}

#[test]
fn hmac_sha256_simple_key_and_data() {
    // Well-known result for key="key", data="The quick brown fox jumps over the lazy dog"
    // HMAC-SHA256 = f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8
    let src = r#"f>t;hmac-sha256 "key" "The quick brown fox jumps over the lazy dog""#;
    for e in ENGINES {
        assert_eq!(
            run_ok(e, src, "f"),
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8",
            "engine={e}"
        );
    }
}

#[test]
fn hmac_sha256_produces_64_char_hex() {
    let src = r#"f>n;len (hmac-sha256 "k" "v")"#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "64", "engine={e}");
    }
}

// ── base64-enc / base64-dec ───────────────────────────────────────────────────

#[test]
fn base64_enc_known_vector() {
    // RFC 4648 §10: base64("Man") = "TWFu"
    let src = r#"f>t;base64-enc "Man""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "TWFu", "engine={e}");
    }
}

#[test]
fn base64_enc_with_padding() {
    // base64("Ma") = "TWE=" (one padding char)
    let src = r#"f>t;base64-enc "Ma""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "TWE=", "engine={e}");
    }
}

#[test]
fn base64_enc_hello() {
    // base64("hello") = "aGVsbG8="
    let src = r#"f>t;base64-enc "hello""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "aGVsbG8=", "engine={e}");
    }
}

#[test]
fn base64_dec_known_vector() {
    // base64-dec("TWFu") = "Man"
    let src = r#"f>R t t;base64-dec "TWFu""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "Man", "engine={e}");
    }
}

#[test]
fn base64_dec_with_padding() {
    let src = r#"f>R t t;base64-dec "TWE=""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "Ma", "engine={e}");
    }
}

#[test]
fn base64_round_trip() {
    let src = r#"f>R t t;base64-dec (base64-enc "hello, world!")"#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "hello, world!", "engine={e}");
    }
}

#[test]
fn base64_dec_invalid_errors() {
    // "!!!" is not valid base64 — should return Err not crash.
    let src = r#"f>t;r=base64-dec "!!!";?r{~_:"ok";^_:"err"}"#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "err", "engine={e}");
    }
}

// ── base64url-enc / base64url-dec ────────────────────────────────────────────

#[test]
fn base64url_enc_no_padding() {
    // base64url("hello") = "aGVsbG8" (no padding, URL-safe alphabet)
    let src = r#"f>t;base64url-enc "hello""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "aGVsbG8", "engine={e}");
    }
}

#[test]
fn base64url_enc_no_plus_or_slash() {
    // base64url uses - and _ instead of + and /. Verify the output never contains
    // standard base64 chars that would be URL-unsafe.
    let src = r#"f>b;s=base64url-enc "hello world this is a test string for url safety";has s "+""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "false", "engine={e}");
    }
    let src2 = r#"f>b;s=base64url-enc "hello world this is a test string for url safety";has s "/""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src2, "f"), "false", "engine={e}");
    }
}

#[test]
fn base64url_round_trip() {
    let src = r#"f>R t t;base64url-dec (base64url-enc "hello, world!")"#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "hello, world!", "engine={e}");
    }
}

#[test]
fn base64url_dec_invalid_errors() {
    let src = r#"f>t;r=base64url-dec "!!!";?r{~_:"ok";^_:"err"}"#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "err", "engine={e}");
    }
}

// ── hex-enc / hex-dec ─────────────────────────────────────────────────────────

#[test]
fn hex_enc_simple() {
    // hex-enc [0, 255, 16] -> "00ff10"
    let src = "f>t;hex-enc [0, 255, 16]";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "00ff10", "engine={e}");
    }
}

#[test]
fn hex_enc_empty_list() {
    let src = "f>t;hex-enc []";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "", "engine={e}");
    }
}

#[test]
fn hex_enc_all_zeros() {
    let src = "f>t;hex-enc [0, 0, 0]";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "000000", "engine={e}");
    }
}

#[test]
fn hex_dec_simple() {
    // hex-dec "00ff10" -> [0, 255, 16]
    let src = r#"f>R (L n) t;hex-dec "00ff10""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "[0, 255, 16]", "engine={e}");
    }
}

#[test]
fn hex_dec_empty_string() {
    let src = r#"f>R (L n) t;hex-dec """#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "[]", "engine={e}");
    }
}

#[test]
fn hex_round_trip() {
    // hex-dec(hex-enc(bytes)) == bytes
    let src = "f>R (L n) t;bytes=[72, 101, 108, 108, 111];hex-dec (hex-enc bytes)";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "[72, 101, 108, 108, 111]", "engine={e}");
    }
}

#[test]
fn hex_dec_uppercase_input() {
    // hex-dec should accept uppercase hex too
    let src = r#"f>R (L n) t;hex-dec "DEADBEEF""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "[222, 173, 190, 239]", "engine={e}");
    }
}

#[test]
fn hex_dec_odd_length_errors() {
    let src = r#"f>t;r=hex-dec "abc";?r{~_:"ok";^_:"err"}"#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "err", "engine={e}");
    }
}

#[test]
fn hex_dec_invalid_chars_errors() {
    let src = r#"f>t;r=hex-dec "zz";?r{~_:"ok";^_:"err"}"#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "err", "engine={e}");
    }
}

#[test]
fn hex_enc_out_of_range_errors() {
    // 256 is not a valid byte value — should error on VM.
    let src = "f>t;hex-enc [256]";
    let stderr = run_err("--vm", src, "f");
    assert!(
        stderr.contains("hex-enc"),
        "VM: expected hex-enc error, got: {stderr}"
    );
}

// ── ct-eq ────────────────────────────────────────────────────────────────────

#[test]
fn ct_eq_equal_strings() {
    let src = r#"f>b;ct-eq "hello" "hello""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "true", "engine={e}");
    }
}

#[test]
fn ct_eq_different_strings() {
    let src = r#"f>b;ct-eq "hello" "world""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "false", "engine={e}");
    }
}

#[test]
fn ct_eq_different_lengths() {
    let src = r#"f>b;ct-eq "hi" "hello""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "false", "engine={e}");
    }
}

#[test]
fn ct_eq_empty_strings() {
    let src = r#"f>b;ct-eq "" """#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "true", "engine={e}");
    }
}

#[test]
fn ct_eq_empty_vs_nonempty() {
    let src = r#"f>b;ct-eq "" "x""#;
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "false", "engine={e}");
    }
}

#[test]
fn ct_eq_hmac_comparison_pattern() {
    // The primary use-case: compare HMAC digests without leaking timing.
    let src = concat!(
        r#"f>b;"#,
        r#"expected=hmac-sha256 "secret" "payload";"#,
        r#"received=hmac-sha256 "secret" "payload";"#,
        r#"ct-eq expected received"#
    );
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "true", "engine={e}");
    }
}

#[test]
fn ct_eq_wrong_type_errors_on_vm() {
    let src = "f>b;ct-eq 42 42";
    let stderr = run_err("--vm", src, "f");
    assert!(
        stderr.contains("ct-eq"),
        "VM: expected ct-eq error, got: {stderr}"
    );
}

// ── integration: sha256 -> hex-enc pipeline ──────────────────────────────────

#[test]
fn sha256_output_is_valid_hex_decodable() {
    // sha256 returns hex — hex-dec should decode it to 32 bytes.
    let src = r#"f>R (L n) t;hex-dec (sha256 "test")"#;
    for e in ENGINES {
        let out = run_ok(e, src, "f");
        // Result is a list of 32 numbers
        assert!(out.starts_with('['), "engine={e}: expected list, got: {out}");
        let count = out.split(',').count();
        assert_eq!(count, 32, "engine={e}: expected 32 bytes, got {count}: {out}");
    }
}
