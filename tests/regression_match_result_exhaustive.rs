// Regression tests for the two-symptom Result-match bug originally reported as:
//
//   mk>R t t;~"got"
//   main>t;r=mk;?r{~v:v;^e:e;_:"x"}   -- returned "x" instead of "got"
//
// Root cause: a bare reference to a zero-arg user function (`mk`) in a value
// position resolved to a function reference (Ty::Fn), not a call. `r` was
// therefore a function value, not a Result, so the `~v:` and `^e:` arms never
// matched at runtime and the wildcard fired. The same misdiagnosis caused the
// verifier to demand a `_:` wildcard (ILO-T024) for what looked like a Result
// subject but was actually Ty::Fn.
//
// The fix auto-expands bare zero-arg user functions into calls (mirroring the
// existing `now`/`now-ms`/`mmap`/`rnd` precedent). After the fix:
//   * `r=mk` calls `mk` and `r` is the Result.
//   * The verifier's existing option-(a) exhaustiveness rule accepts the
//     canonical two-arm form `?r{~v:v;^e:e}` with no wildcard required.
//   * Runtime returns "got" — the wildcard, when present, never preempts
//     specific arms because `r` is a real Result value.
//
// These tests pin the behaviour across every execution engine so the fix
// cannot silently regress on any backend.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm"];

// Canonical originating repro: zero-arg fn `mk` returning Ok, matched with
// both specific arms AND a trailing wildcard. Specific arm must win.
const ZERO_ARG_OK_WITH_WILDCARD: &str = r#"mk>R t t;~"got"
main>t;r=mk;?r{~v:v;^e:e;_:"x"}"#;

#[test]
fn zero_arg_result_ok_specific_arm_wins_over_wildcard_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, ZERO_ARG_OK_WITH_WILDCARD, "main");
        assert_eq!(out.trim(), "got", "{engine}");
    }
}

// Canonical two-arm Result match (option (a)): no wildcard, must verify AND
// run correctly across every engine. Zero-arg fn so we exercise the bare-ref
// auto-call path.
const ZERO_ARG_OK_TWO_ARM: &str = r#"mk>R t t;~"got"
main>t;r=mk;?r{~v:v;^e:e}"#;

#[test]
fn zero_arg_result_two_arm_exhaustive_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, ZERO_ARG_OK_TWO_ARM, "main");
        assert_eq!(out.trim(), "got", "{engine}");
    }
}

// Mirror of the above for the Err branch — zero-arg fn returning `^"boom"`.
const ZERO_ARG_ERR_TWO_ARM: &str = r#"mk>R t t;^"boom"
main>t;r=mk;?r{~v:v;^e:e}"#;

#[test]
fn zero_arg_result_two_arm_err_branch_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, ZERO_ARG_ERR_TWO_ARM, "main");
        assert_eq!(out.trim(), "boom", "{engine}");
    }
}

// Multi-arg fn version — option (a) was always meant to cover this; pin it
// cross-engine so the verifier change can never silently regress.
const MULTI_ARG_RESULT_TWO_ARM: &str = r#"div a:n b:n>R n t;=b 0 ^"zero";~/a b
main>t;r=div 10 2;?r{~v:str v;^e:e}"#;

#[test]
fn multi_arg_result_two_arm_exhaustive_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, MULTI_ARG_RESULT_TWO_ARM, "main");
        assert_eq!(out.trim(), "5", "{engine}");
    }
}

// Verifier-level check: the canonical two-arm Result match must not emit
// ILO-T024. Use `ilo run` and assert exit success — verify happens before
// execution.
#[test]
fn verifier_accepts_two_arm_result_match() {
    let out = ilo()
        .args([ZERO_ARG_OK_TWO_ARM, "main"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "verifier rejected two-arm Result match: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "got");
}
