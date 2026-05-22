// Cross-engine regression tests for `sha256-hex` and `sha256d` (ILO-383).
//
// Both builtins hash hex-decoded raw bytes rather than UTF-8 text:
//   - `sha256-hex hex:t > t` — SHA-256 of hex-decoded bytes, lowercase hex.
//   - `sha256d hex:t > t` — double-SHA256 (sha256(sha256(x))), lowercase hex.
//     Bitcoin Merkle tree protocol shape.
//
// All tests fan across VM and JIT to pin tree-bridge dispatch. Vectors are
// derived from NIST FIPS-180 (sha256-hex of "abc" bytes) and manual
// computation of sha256d. Error-path tests confirm ILO-R009 surfaces
// correctly for odd-length and non-hex inputs.

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
    // These should exit non-zero (runtime error)
    String::from_utf8_lossy(&out.stderr).to_string()
}

// ── sha256-hex ────────────────────────────────────────────────────────────────

#[test]
fn sha256_hex_of_abc_bytes() {
    // "616263" is hex for "abc". NIST FIPS-180 anchor: SHA-256("abc") =
    // ba7816...15ad. The raw-bytes path must agree with the UTF-8 path for
    // ASCII inputs — cross-validates sha256-hex against the existing sha256.
    let src = "f>t;sha256-hex \"616263\"";
    let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), expected, "engine={e}");
    }
}

#[test]
fn sha256_hex_of_empty_bytes() {
    // Empty hex string = empty byte slice. SHA-256(empty) = e3b0c4...b855.
    // NIST FIPS-180 anchor; also validates that zero-length input is legal.
    let src = "f>t;sha256-hex \"\"";
    let expected = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), expected, "engine={e}");
    }
}

#[test]
fn sha256_hex_output_is_64_chars() {
    // Every SHA-256 digest is exactly 32 bytes → 64 lowercase hex chars.
    let src = "f>n;len (sha256-hex \"deadbeef\")";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "64", "engine={e}");
    }
}

#[test]
fn sha256_hex_uppercase_input_accepted() {
    // Hex decoder must accept uppercase A-F in addition to lowercase a-f.
    // "DEADBEEF" and "deadbeef" decode to the same bytes.
    let src_upper = "f>t;sha256-hex \"DEADBEEF\"";
    let src_lower = "f>t;sha256-hex \"deadbeef\"";
    for e in ENGINES {
        let upper = run_ok(e, src_upper, "f");
        let lower = run_ok(e, src_lower, "f");
        assert_eq!(upper, lower, "engine={e}: uppercase vs lowercase input");
    }
}

#[test]
fn sha256_hex_odd_length_errors() {
    // Odd-length hex is undecodable (half-byte at the end). Must surface
    // ILO-R009, not panic or silently produce garbage.
    let src = "f>t;sha256-hex \"abc\""; // 3 chars = odd
    for e in ENGINES {
        let stderr = run_err(e, src, "f");
        assert!(
            stderr.contains("ILO-R009"),
            "engine={e}: expected ILO-R009 in stderr, got: {stderr}"
        );
    }
}

#[test]
fn sha256_hex_non_hex_input_errors() {
    // Non-hex characters must surface ILO-R009.
    let src = "f>t;sha256-hex \"zz\""; // 'z' is not a hex digit
    for e in ENGINES {
        let stderr = run_err(e, src, "f");
        assert!(
            stderr.contains("ILO-R009"),
            "engine={e}: expected ILO-R009 in stderr, got: {stderr}"
        );
    }
}

// ── sha256d ───────────────────────────────────────────────────────────────────

#[test]
fn sha256d_of_abc_bytes() {
    // sha256d("abc" bytes) = sha256(sha256(0x61 0x62 0x63)).
    // Known vector: 4f8b42c2...6358.
    let src = "f>t;sha256d \"616263\"";
    let expected = "4f8b42c22dd3729b519ba6f68d2da7cc5b2d606d05daed5ad5128cc03e6c6358";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), expected, "engine={e}");
    }
}

#[test]
fn sha256d_equals_double_sha256_hex() {
    // sha256d(x) must equal sha256-hex(sha256-hex(x)) for any input.
    // This pins the composition contract: the named form is sugar for the
    // composed form; any divergence between them is a bug.
    let src = "f>b;= (sha256d \"deadbeef\") (sha256-hex (sha256-hex \"deadbeef\"))";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "true", "engine={e}");
    }
}

#[test]
fn sha256d_bitcoin_merkle_two_txids() {
    // Bitcoin Merkle root of two txids (hex-concatenated, internal byte order).
    // sha256d(tx1 || tx2) is the standard Bitcoin 2-leaf Merkle computation.
    // Vector: Block #170 txid pair → cba2bf79...38bb.
    let src = concat!(
        "f>t;",
        "tx1=\"0e3e2357e806b6cdb1f70b54c3a3a17b6714ee1f0e68bebb44a74b1efd512098\";",
        "tx2=\"169e1e83e930853391bc6f35f605c6754cfead57cf8387639d3b4096c54f18f4\";",
        "sha256d (+ tx1 tx2)"
    );
    let expected = "cba2bf79b14c6ff80108eb1a4d2f4b55cf4776dcdea24f292ba7dd163b5338bb";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), expected, "engine={e}");
    }
}

#[test]
fn sha256d_odd_length_errors() {
    // Odd-length hex must surface ILO-R009.
    let src = "f>t;sha256d \"abc\"";
    for e in ENGINES {
        let stderr = run_err(e, src, "f");
        assert!(
            stderr.contains("ILO-R009"),
            "engine={e}: expected ILO-R009 in stderr, got: {stderr}"
        );
    }
}

#[test]
fn sha256d_non_hex_input_errors() {
    // Non-hex characters must surface ILO-R009.
    let src = "f>t;sha256d \"gg\"";
    for e in ENGINES {
        let stderr = run_err(e, src, "f");
        assert!(
            stderr.contains("ILO-R009"),
            "engine={e}: expected ILO-R009 in stderr, got: {stderr}"
        );
    }
}
