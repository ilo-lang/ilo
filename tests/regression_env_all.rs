// Cross-engine regression tests for the `env-all` builtin.
//
// `env-all > R M t t` snapshots the full process environment as a
// Map[Text, Text] wrapped in `R`. Originating friction: env-config
// rerun1 in `ilo_assessment_feedback.md` — without enumeration, agents
// had to hardcode key lists for "merge env over config" patterns.
//
// Every test runs through tree / VM / Cranelift to pin cross-engine
// parity: the tree-bridge dispatch path means the JIT and bytecode VM
// both route through `call_builtin_for_bridge_with_program`, and any
// drift between the engines would show up as a divergent value here.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--vm"];

/// A canary key that must exist for these tests to be meaningful. We
/// set it ourselves via Command::env so the test doesn't depend on
/// the host shell's environment (CI sandboxes can be sparse).
const CANARY_KEY: &str = "ILO_ENV_ALL_TEST_CANARY";
const CANARY_VALUE: &str = "canary-value-42";

fn run(engine: &str, src: &str, fn_name: &str, set_canary: bool) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine).arg(fn_name);
    if set_canary {
        cmd.env(CANARY_KEY, CANARY_VALUE);
    } else {
        cmd.env_remove(CANARY_KEY);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} {src:?} {fn_name} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .trim_end_matches('\n')
        .to_string()
}

// `env-all` must return Ok(map) on every engine. We assert by looking up
// the canary we set on the child process: `mget m KEY` returns the value
// when present, nil when missing. `m=env-all!` auto-unwraps the outer
// `R`, so a non-Ok return would fail the auto-unwrap and the engine
// would exit nonzero, tripping the run() assertion.
const CANARY_PRESENT_SRC: &str =
    "f>R t t;m=env-all!;v=mget m \"ILO_ENV_ALL_TEST_CANARY\";?v{nil:^\"missing\";_:~v}";

#[test]
fn env_all_returns_map_with_canary_set() {
    for engine in ENGINES_ALL {
        let out = run(engine, CANARY_PRESENT_SRC, "f", true);
        assert_eq!(
            out, CANARY_VALUE,
            "{engine}: env-all should expose the canary the test set on the child"
        );
    }
}

// Value parity with `env key`: any key returned by env-all must round-trip
// against `env key` calls. PATH is the most reliable always-set var; on
// the off chance it isn't, fall back to the canary. The test sets the
// canary so PATH-or-canary is guaranteed.
const PARITY_SRC: &str =
    "f k:t>R t t;m=env-all!;a=mget m k;b=env! k;?=a b ~\"match\" ^\"mismatch\"";

#[test]
fn env_all_value_matches_env_key() {
    for engine in ENGINES_ALL {
        let out = run_with_arg(engine, PARITY_SRC, "f", CANARY_KEY);
        assert_eq!(
            out, "match",
            "{engine}: env-all[k] must equal env(k) for the same k"
        );
    }
}

fn run_with_arg(engine: &str, src: &str, fn_name: &str, arg: &str) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine).arg(fn_name).arg(arg);
    cmd.env(CANARY_KEY, CANARY_VALUE);
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} {src:?} {fn_name} {arg} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .trim_end_matches('\n')
        .to_string()
}

// Empty-env behaviour: even with the canary removed, env-all should
// return Ok (not Err) — std::env::vars never fails. Returning the len
// rather than the map itself keeps the test deterministic without
// asserting on the host's specific env vars. A non-Ok result would
// crash the auto-unwrap and fail run().
const SIZE_SRC: &str = "f>R n t;m=env-all!;~len m";

#[test]
fn env_all_returns_ok_even_when_canary_absent() {
    for engine in ENGINES_ALL {
        let out = run(engine, SIZE_SRC, "f", false);
        // We don't pin the exact count (host varies). The fact that we
        // got a number proves the auto-unwrap succeeded and the map
        // round-tripped through every engine's tree-bridge path.
        let n: i64 = out
            .parse()
            .unwrap_or_else(|_| panic!("{engine}: expected number, got {out:?}"));
        assert!(n >= 0, "{engine}: env-all len must be non-negative");
    }
}

// mkeys over env-all must return a non-empty L t in normal environments
// (and at minimum contains the canary we set). Pins that the map's
// keys are valid `t` values, not numbers or other types — a regression
// in the MapKey -> Text round-trip would show up here.
const KEYS_HAS_CANARY_SRC: &str =
    "f>R b t;m=env-all!;ks=mkeys m;~has ks \"ILO_ENV_ALL_TEST_CANARY\"";

#[test]
fn env_all_keys_contains_canary() {
    for engine in ENGINES_ALL {
        let out = run(engine, KEYS_HAS_CANARY_SRC, "f", true);
        assert_eq!(
            out, "true",
            "{engine}: mkeys(env-all) must include the canary key as text"
        );
    }
}

// Bare `env-all` (without `!`) in expression position must parse as a
// zero-arg Call, not a Ref to an undefined variable. Mirrors the
// `now` / `now-ms` / `mmap` zero-arg parse precedent in parse_atom /
// parse_pratt. The verifier then sees a Result-returning call and
// downstream consumers (mkeys, len, mget on the Ok arm) type-check
// against the inner `M t t`.
const BARE_CALL_SRC: &str = "f>R n t;r=env-all;?r{~m:~len m;^er:^e}";

#[test]
fn env_all_bare_call_parses_as_zero_arg() {
    for engine in ENGINES_ALL {
        let out = run(engine, BARE_CALL_SRC, "f", true);
        let n: i64 = out
            .parse()
            .unwrap_or_else(|_| panic!("{engine}: expected number, got {out:?}"));
        assert!(n >= 1, "{engine}: env-all len must be >=1 (canary is set)");
    }
}

// Arity guard: env-all takes no args. Passing one must error at verify,
// not silently dispatch. Mirrors the `now` / `now-ms` arity tests.
#[test]
fn env_all_rejects_extra_arg() {
    for engine in ENGINES_ALL {
        let mut cmd = ilo();
        cmd.arg("f>R t t;m=env-all! \"extra\";~\"x\"")
            .arg(engine)
            .arg("f");
        let out = cmd.output().expect("failed to run ilo");
        assert!(
            !out.status.success(),
            "{engine}: env-all with an extra arg should error, but succeeded with stdout={}",
            String::from_utf8_lossy(&out.stdout)
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("env-all") || stderr.contains("arity") || stderr.contains("ILO"),
            "{engine}: expected an arity / verifier diagnostic mentioning env-all, got: {stderr}"
        );
    }
}
