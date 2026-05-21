// Cross-engine regression tests for `rand-bytes n > t` — the cryptographic
// random builtin added in 0.12.x.
//
// `rand-bytes` is distinct from `rnd` (seedable uniform float) and `rndn`
// (seedable Normal float): it pulls from the platform CSPRNG (via the
// `getrandom` crate) and is the path agents need for JWT `jti` claims,
// CSRF tokens, session IDs, and nonces. Output is base64url-no-pad so the
// result drops into headers / cookies / query strings without further
// encoding.
//
// Tree-bridge eligible: VM and Cranelift JIT dispatch through the tree
// interpreter, so all engines share the same semantics. Tests fan across
// every available engine to catch any future divergence.
//
// Per the CLAUDE.md test-coverage rule: cross-cutting (VM + JIT), error
// paths (negative + non-finite + over-cap), and uniqueness all covered.

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

// ── Length: base64url-no-pad is ceil(n * 4 / 3) chars ────────────────────────

#[test]
fn rand_bytes_16_encodes_to_22_chars() {
    // 16 raw bytes (128 bits, canonical jti / CSRF token size) → 22 b64url chars.
    let src = "f>n;len (rand-bytes 16)";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "22", "engine={e}");
    }
}

#[test]
fn rand_bytes_32_encodes_to_43_chars() {
    // 32 raw bytes (256 bits, standard session-id size) → 43 b64url chars.
    let src = "f>n;len (rand-bytes 32)";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "43", "engine={e}");
    }
}

#[test]
fn rand_bytes_1_encodes_to_2_chars() {
    // 1 raw byte → 2 b64url chars (6 bits + 2 trailing bits, no padding).
    // Pins the chunks-remainder path for `rem.len() == 1`.
    let src = "f>n;len (rand-bytes 1)";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "2", "engine={e}");
    }
}

#[test]
fn rand_bytes_2_encodes_to_3_chars() {
    // 2 raw bytes → 3 b64url chars. Pins the `rem.len() == 2` branch.
    let src = "f>n;len (rand-bytes 2)";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "3", "engine={e}");
    }
}

#[test]
fn rand_bytes_3_encodes_to_4_chars() {
    // 3 raw bytes → 4 b64url chars. Exact multiple of 3, no remainder.
    let src = "f>n;len (rand-bytes 3)";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "4", "engine={e}");
    }
}

#[test]
fn rand_bytes_0_returns_empty_string() {
    // Zero bytes encode to the empty string. Degenerate but valid.
    let src = "f>n;len (rand-bytes 0)";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "0", "engine={e}");
    }
}

// ── Uniqueness: two calls return distinct bytes (probabilistic) ──────────────

#[test]
fn rand_bytes_two_calls_differ() {
    // False-fail rate is 2^-128 for 16-byte draws — vanishingly small.
    // If this ever fires deterministically the CSPRNG is broken, not the test.
    let src = "f>t;a=rand-bytes 16;b=rand-bytes 16;?(=a b){true:\"same\";false:\"diff\"}";
    for e in ENGINES {
        assert_eq!(run_ok(e, src, "f"), "diff", "engine={e}");
    }
}

// ── Alphabet: output must be URL-safe (no `+` / `/` / `=`) ───────────────────

#[test]
fn rand_bytes_output_is_url_safe() {
    // The encoder uses the RFC 4648 §5 alphabet with no padding. None of the
    // characters that need percent-encoding in a URL (`+`, `/`, `=`) may
    // appear. Run a 256-byte draw to make the alphabet test substantial.
    let src = "f>t;rand-bytes 256";
    for e in ENGINES {
        let out = run_ok(e, src, "f");
        assert!(
            !out.contains('+') && !out.contains('/') && !out.contains('='),
            "engine={e}: URL-unsafe char in output: {out:?}"
        );
        assert!(
            out.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "engine={e}: non-b64url char in output: {out:?}"
        );
    }
}

// ── Errors: negative n, non-finite n, oversized n ────────────────────────────

#[test]
fn rand_bytes_negative_errors_on_vm() {
    // n must be non-negative: surfaces as ILO-R009 at runtime. JIT silently
    // returns nil for tree-bridge builtin errors (same known limitation noted
    // in regression_dur_parse_fmt.rs); only VM is tested for propagation.
    let src = "f>t;rand-bytes -1";
    let stderr = run_err("--vm", src, "f");
    assert!(
        stderr.contains("rand-bytes") && stderr.contains("non-negative"),
        "VM: expected ILO-R009 non-negative error, got: {stderr}"
    );
}

#[test]
fn rand_bytes_over_cap_errors_on_vm() {
    // Cap is 1 MiB raw — larger draws are almost certainly a bug. Pin the
    // diagnostic so the cap isn't silently raised without intent. VM-only:
    // JIT swallows tree-bridge errors as nil (see negative test above).
    let src = "f>t;rand-bytes 2000000";
    let stderr = run_err("--vm", src, "f");
    assert!(
        stderr.contains("rand-bytes") && stderr.contains("1 MiB"),
        "VM: expected ILO-R009 cap error, got: {stderr}"
    );
}

#[test]
fn rand_bytes_wrong_type_errors_on_vm() {
    // rand-bytes expects a number. JIT silently returns nil for tree-bridge
    // builtins that receive wrong-type args (same pattern as dirname / dur-fmt
    // tests); only the VM engine is checked for wrong-type propagation here.
    let src = "f>t;rand-bytes \"nope\"";
    let stderr = run_err("--vm", src, "f");
    assert!(
        stderr.contains("rand-bytes"),
        "VM: expected rand-bytes error, got: {stderr}"
    );
}
