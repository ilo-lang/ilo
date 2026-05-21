// Cross-engine regression tests for the crypto-primitives cluster:
// `sha256`, `hmac-sha256`, `b64`, `b64-dec`, `hex`, `ct-eq`.
//
// All six builtins are tree-bridge eligible (pure text-in / text-or-bool-out,
// no FnRef args, no I/O), so VM and Cranelift JIT dispatch through the tree
// interpreter. The point of fanning every assertion across `--vm` and
// `--jit` is to catch any future divergence in the bridge dispatch — these
// are the path agents use for HMAC signature verification (webhook auth)
// and JWT signing, so silent cross-engine drift would be expensive.
//
// Test vectors are pinned to the relevant RFCs so future implementation
// swaps trip the test, not a downstream agent:
//   - SHA-256: NIST FIPS-180 anchor vectors (empty string, "abc")
//   - HMAC-SHA256: RFC 4231 test case 2 (printable ASCII key + message)
//   - base64: RFC 4648 §4 (standard alphabet, `=` padding)
//   - hex: lowercase output, every byte → exactly 2 chars
//   - ct-eq: equal / unequal-same-length / unequal-different-length

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

// ── SHA-256 (NIST FIPS-180 anchor vectors) ───────────────────────────────────

#[test]
fn sha256_empty_string() {
    // FIPS-180 anchor: SHA-256("") = e3b0c4...b855. If this ever drifts,
    // every signature-verification call site silently produces wrong output.
    let src = "f>t;sha256 \"\"";
    let expected = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), expected, "engine={e}");
    }
}

#[test]
fn sha256_abc() {
    // FIPS-180 anchor: SHA-256("abc") = ba7816...15ad.
    let src = "f>t;sha256 \"abc\"";
    let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), expected, "engine={e}");
    }
}

#[test]
fn sha256_length_is_64_hex_chars() {
    // Every SHA-256 digest is exactly 32 bytes → 64 lowercase hex chars.
    // Pins the encoding contract that callers rely on (e.g. fixed-width
    // database columns, hex-comparing against a known prefix).
    let src = "f>n;len (sha256 \"hello world\")";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "64", "engine={e}");
    }
}

// ── HMAC-SHA256 (RFC 4231 test case 2) ───────────────────────────────────────

#[test]
fn hmac_sha256_rfc4231_case_2() {
    // RFC 4231 §4.3: key "Jefe", data "what do ya want for nothing?".
    // Chosen because both inputs are printable ASCII (no `\x` escapes
    // needed, which the ilo lexer doesn't support).
    let src = "f>t;hmac-sha256 \"Jefe\" \"what do ya want for nothing?\"";
    let expected = "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), expected, "engine={e}");
    }
}

#[test]
fn hmac_sha256_empty_key_empty_message() {
    // HMAC-SHA256("", "") = b613679a...5c20. Documented widely; an
    // independent reference vector to catch any padding regressions.
    let src = "f>t;hmac-sha256 \"\" \"\"";
    let expected = "b613679a0814d9ec772f95d778c35fc5ff1697c493715653c6c712144292c5ad";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), expected, "engine={e}");
    }
}

// ── base64 standard alphabet (RFC 4648 §4) ───────────────────────────────────

#[test]
fn b64_encode_roundtrip_foobar() {
    // RFC 4648 §10: "foobar" -> "Zm9vYmFy" (multiple of 3 bytes, no padding).
    let src = "f>t;b64 \"foobar\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "Zm9vYmFy", "engine={e}");
    }
}

#[test]
fn b64_encode_pads_one_byte() {
    // "M" → "TQ==" (1 byte input, 2 `=` padding chars). Distinct from
    // b64u which strips padding.
    let src = "f>t;b64 \"M\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "TQ==", "engine={e}");
    }
}

#[test]
fn b64_encode_pads_two_bytes() {
    // "Ma" → "TWE=" (2 byte input, 1 `=` padding char).
    let src = "f>t;b64 \"Ma\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "TWE=", "engine={e}");
    }
}

#[test]
fn b64_dec_roundtrip() {
    // b64-dec is the inverse of b64. `!` auto-unwraps the R t t.
    let src = "f>t;b64-dec! (b64 \"hello world\")";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "hello world", "engine={e}");
    }
}

#[test]
fn b64_dec_known_vector() {
    // "Zm9vYmFy" → "foobar". Pins the decode alphabet — if anyone swaps
    // the standard alphabet for url-safe (`-`/`_`) this breaks.
    let src = "f>t;b64-dec! \"Zm9vYmFy\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "foobar", "engine={e}");
    }
}

#[test]
fn b64_dec_invalid_input_returns_err() {
    // Use `default-on-err` to surface a fixed sentinel so we can assert
    // on stdout deterministically across engines. `??` is the nil-coalesce
    // operator (for `O T`); `default-on-err` is its Result counterpart.
    //
    // Input "!!!!" is outside the standard base64 alphabet — must Err.
    let src = "f>t;default-on-err (b64-dec \"!!!!\") \"got-err\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "got-err", "engine={e}");
    }
}

// ── hex ──────────────────────────────────────────────────────────────────────

#[test]
fn hex_encode_abc() {
    // "abc" → "616263". Three ASCII bytes, six lowercase hex chars.
    let src = "f>t;hex \"abc\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "616263", "engine={e}");
    }
}

#[test]
fn hex_encode_empty() {
    // Empty input → empty output. Pins the no-allocation degenerate path.
    let src = "f>t;hex \"\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "", "engine={e}");
    }
}

#[test]
fn hex_encode_length_doubles() {
    // Every UTF-8 byte produces exactly 2 hex chars. ASCII "hello" → 10 chars.
    let src = "f>n;len (hex \"hello\")";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "10", "engine={e}");
    }
}

#[test]
fn hex_encode_non_ascii() {
    // UTF-8 multi-byte char: "ñ" is 0xC3 0xB1 → "c3b1". Confirms we encode
    // raw UTF-8 bytes, not codepoints.
    let src = "f>t;hex \"ñ\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "c3b1", "engine={e}");
    }
}

// ── ct-eq ────────────────────────────────────────────────────────────────────

#[test]
fn ct_eq_equal_inputs() {
    let src = "f>b;ct-eq \"secret123\" \"secret123\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "true", "engine={e}");
    }
}

#[test]
fn ct_eq_unequal_same_length() {
    // Same length, last char differs. Pins the constant-time scan path:
    // a naive `==` short-circuits on the first mismatch; ct-eq must not.
    let src = "f>b;ct-eq \"secret123\" \"secret124\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "false", "engine={e}");
    }
}

#[test]
fn ct_eq_different_length() {
    // Different lengths short-circuit to false (length isn't secret in
    // any realistic protocol — HMAC digests are fixed-size, tokens too).
    let src = "f>b;ct-eq \"short\" \"longer\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "false", "engine={e}");
    }
}

#[test]
fn ct_eq_empty_strings() {
    let src = "f>b;ct-eq \"\" \"\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "true", "engine={e}");
    }
}

// ── Tree-bridge auto-unwrap invariant for b64-dec! ───────────────────────────

#[test]
fn b64_dec_bang_auto_unwrap_does_not_panic_vm() {
    // Regression for the 0.12.x crypto-cluster panic where `b64-dec!`
    // crashed the VM with "auto-unwrap on a non-Result tree-bridge builtin
    // slipped past verify" at src/vm/mod.rs:1887. `b64-dec` returns R t t
    // and is tree-bridge eligible, so it must also appear in
    // `tree_bridge_returns_result` for the VM's `!` compiler arm to wire
    // up OP_ISOK/OP_UNWRAP correctly. The matching tree-bridge / verifier
    // pair for B64uDec was already correct; B64Dec was the regression.
    //
    // This test specifically pins the VM `!` path so a future drop of
    // B64Dec from `tree_bridge_returns_result` is caught immediately
    // instead of waiting for the `examples` harness to hit it.
    let src = "f>t;b64-dec! \"Zm9vYmFy\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "foobar", "engine={e}");
    }
}

#[test]
fn b64_dec_bang_propagates_err() {
    // Companion to the panic-regression test above: the `!` operator
    // propagates Err out to the caller (which we trigger as a non-zero
    // exit), so an invalid input must still take the propagate path
    // cleanly across engines, not panic.
    let src = "f>t;b64-dec! \"!!!!\"";
    for e in ENGINES {
        let out = ilo()
            .args([src, e, "f"])
            .output()
            .expect("failed to run ilo");
        assert!(
            !out.status.success(),
            "engine={e}: expected non-zero exit from b64-dec! on invalid input"
        );
        // Must NOT be the VM bridge-assert panic.
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !stderr.contains("auto-unwrap on a non-Result tree-bridge builtin slipped past verify"),
            "engine={e}: VM tree-bridge assert tripped; stderr={stderr}"
        );
    }
}

// ── HMAC verification flow (the canonical use case) ──────────────────────────

#[test]
fn hmac_verification_roundtrip_uses_ct_eq() {
    // The intended call site: compute hmac-sha256, compare against a known
    // signature via ct-eq. This pattern is what webhook receivers and
    // request signers need; pinning it cross-engine guards the path that
    // matters most operationally.
    let src = "f>b;sig=hmac-sha256 \"key\" \"payload\";\
               ct-eq sig \"5d98b45c90a207fa998ce639fea6f02ecc8cc3f36fef81d694fb856b4d0a28ca\"";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "true", "engine={e}");
    }
}
