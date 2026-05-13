// Regression coverage for `mget m k ?? d` — one-line map-lookup with default.
//
// Older persona reports (lines 699, 705, 926 of ilo_assessment_feedback.md)
// claimed `mget m k??0` parsed as `mget(m, k??0)`, forcing a two-step
// `r=mget m k;v=r??0` bind. The arity-aware call parser landed in 06477c5
// fixed this — `mget` is registered with fixed arity 2, so the parser
// stops consuming args after the key and `??` falls out as infix
// nil-coalesce on the whole call result.
//
// These tests pin that behaviour across every engine (tree, VM,
// Cranelift) and across the variants personas actually wrote: literal
// key, variable key, path-access key, call-result key, parenthesised
// key, with both numeric and text defaults.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_file(engine: &str, src: &str, entry: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "ilo_mget_default_{}_{}.ilo",
        std::process::id(),
        seq
    ));
    std::fs::write(&path, src).unwrap();
    let out = ilo()
        .args([path.to_str().unwrap(), engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// Literal key, value present — `(mget m "k") ?? 0` → 5.
const LITERAL_HIT: &str = r#"f>n;m=mset mmap "k" 5;mget m "k" ?? 0"#;
// Literal key, value missing — `(mget m "missing") ?? 99` → 99.
const LITERAL_MISS: &str = r#"f>n;m=mmap;mget m "missing" ?? 99"#;
// Variable key, value present.
const VARKEY_HIT: &str = r#"f>n;m=mset mmap "k" 5;k="k";mget m k ?? 0"#;
// Variable key, value missing (the exact shape from persona line 699).
const VARKEY_MISS: &str = r#"f>n;em=mmap;k="x";mget em k ?? 0"#;
// Text-typed default, text value present.
const TEXT_HIT: &str = r#"f>t;m=mset mmap "k" "hi";mget m "k" ?? "default""#;
// Text default reached on missing key — confirms `??` typing flows
// through `mget`'s `O T` return.
const TEXT_MISS: &str = r#"f>t;m=mmap;mget m "absent" ?? "default""#;
// Path-access key (`ks.0`) — confirms the parser doesn't greedily
// swallow `.0` or `?? 0` into the key expression.
const PATHKEY_HIT: &str = r#"f>n;m=mset mmap "k" 5;ks=["k"];mget m ks.0 ?? 0"#;
// Call-result key (`str 5`) — confirms the parser stops the key
// expression at mget's second argument boundary, not at `??`.
const CALLKEY_HIT: &str = r#"f>n;m=mset mmap "5" 7;mget m str 5 ?? 0"#;
// Parenthesised key — defensive lower bound on the precedence.
const PARENKEY_HIT: &str = r#"f>n;m=mset mmap "5" 7;mget m (str 5) ?? 0"#;

// --- chained `mget m k ?? mget m k2 ?? d` ---
//
// Earlier, the post-arg break in `parse_call_or_atom` let `??` through
// when followed by 2+ atoms (the prefix-binary lookahead matched), so
// `mget m "a" ?? mget m "b" ?? 99` parsed as `mget m "a" (?? mget m) (?? "b" ...)`
// and failed with ILO-T006 "expects 2 args, got 3". Now `??` is always
// infix once at least one call arg has been collected.

// First lookup hits — short-circuits before second `mget` runs.
const CHAIN_FIRST_HIT: &str = r#"f>n;m=mset mmap "a" 1;mget m "a" ?? mget m "b" ?? 99"#;
// First miss, second hits.
const CHAIN_SECOND_HIT: &str = r#"f>n;m=mset mmap "b" 2;mget m "a" ?? mget m "b" ?? 99"#;
// Both miss — default wins.
const CHAIN_BOTH_MISS: &str = r#"f>n;m=mmap;mget m "a" ?? mget m "b" ?? 99"#;
// Two-element chain with no trailing default.
const CHAIN_NO_DEFAULT_HIT: &str = r#"f>O n;m=mset mmap "a" 1;mget m "a" ?? mget m "b""#;
const CHAIN_NO_DEFAULT_MISS: &str = r#"f>O n;m=mset mmap "b" 2;mget m "a" ?? mget m "b""#;
// `at` is another arity-2 builtin — confirm the fix isn't `mget`-specific.
// (Skip the out-of-bounds case: engines disagree on whether `at` returns nil
// or raises ILO-R004/ILO-R009, which is orthogonal to the parser fix.)
const CHAIN_AT_FIRST_HIT: &str = r#"f>n;xs=[10 20 30];at xs 0 ?? at xs 1 ?? 0"#;
const CHAIN_AT_SECOND_HIT: &str = r#"f>n;xs=[10 20 30];at xs 1 ?? at xs 2 ?? 0"#;

fn check_all(engine: &str) {
    assert_eq!(
        run_file(engine, LITERAL_HIT, "f"),
        "5",
        "literal hit engine={engine}"
    );
    assert_eq!(
        run_file(engine, LITERAL_MISS, "f"),
        "99",
        "literal miss engine={engine}"
    );
    assert_eq!(
        run_file(engine, VARKEY_HIT, "f"),
        "5",
        "varkey hit engine={engine}"
    );
    assert_eq!(
        run_file(engine, VARKEY_MISS, "f"),
        "0",
        "varkey miss engine={engine}"
    );
    assert_eq!(
        run_file(engine, TEXT_HIT, "f"),
        "hi",
        "text hit engine={engine}"
    );
    assert_eq!(
        run_file(engine, TEXT_MISS, "f"),
        "default",
        "text miss engine={engine}"
    );
    assert_eq!(
        run_file(engine, PATHKEY_HIT, "f"),
        "5",
        "pathkey hit engine={engine}"
    );
    assert_eq!(
        run_file(engine, CALLKEY_HIT, "f"),
        "7",
        "callkey hit engine={engine}"
    );
    assert_eq!(
        run_file(engine, PARENKEY_HIT, "f"),
        "7",
        "parenkey hit engine={engine}"
    );
    assert_eq!(
        run_file(engine, CHAIN_FIRST_HIT, "f"),
        "1",
        "chain first hit engine={engine}"
    );
    assert_eq!(
        run_file(engine, CHAIN_SECOND_HIT, "f"),
        "2",
        "chain second hit engine={engine}"
    );
    assert_eq!(
        run_file(engine, CHAIN_BOTH_MISS, "f"),
        "99",
        "chain both miss engine={engine}"
    );
    assert_eq!(
        run_file(engine, CHAIN_NO_DEFAULT_HIT, "f"),
        "1",
        "chain no-default hit engine={engine}"
    );
    assert_eq!(
        run_file(engine, CHAIN_NO_DEFAULT_MISS, "f"),
        "2",
        "chain no-default miss engine={engine}"
    );
    assert_eq!(
        run_file(engine, CHAIN_AT_FIRST_HIT, "f"),
        "10",
        "chain at first hit engine={engine}"
    );
    assert_eq!(
        run_file(engine, CHAIN_AT_SECOND_HIT, "f"),
        "20",
        "chain at second hit engine={engine}"
    );
}

#[test]
fn mget_default_tree() {
    check_all("--run-tree");
}

#[test]
fn mget_default_vm() {
    check_all("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn mget_default_cranelift() {
    check_all("--run-cranelift");
}
