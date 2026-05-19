// Regression: `jpth` returns typed ilo values for array / object / scalar
// leaves, and the new `jkeys` builtin enumerates the top-level keys of a
// JSON object at a dot-path.
//
// Originating friction (2026-05-19):
//   - event-trace-analyser rerun10 — had to reshape `{"spans":[...]}` to JSONL
//     because `jpth blob "spans"` returned the stringified array text, which
//     `flt` / `map` / `@` then rejected with a giant "got Text(...)" stderr.
//   - monorepo-analyst rerun10 — `jpth pkg "dependencies"` returned a stringy
//     JSON blob, `mkeys` rejected it (`'mkeys' expects a map, got t`), forced
//     a shell-out to jq for the actual key enumeration.
//
// Root cause: tree (`src/interpreter/mod.rs`), VM (`src/vm/mod.rs` OP_JPTH),
// Cranelift (`src/vm/mod.rs` jit_jpth), and Python codegen all stringified
// the resolved JSON value via `other.to_string()` instead of returning the
// typed Value via `serde_json_to_value` / `serde_json_to_nanval`.
//
// Fix (0.12.1):
//   - jpth now returns `R _ t`. The Ok variant holds whatever ilo type the
//     JSON value maps to: array → list, object → record, string → text,
//     number → number, bool → bool, null → nil.
//   - New `jkeys j:t p:t > R (L t) t` returns the sorted top-level keys of
//     the JSON object at the dot-path. Empty path = root. Err when the value
//     at the path is not an object (e.g. an array, scalar, or null).
//
// Engines exercised: tree, VM, Cranelift JIT. AOT goes through the same
// `jit_jpth` extern "C" helper as the JIT, so the JIT row covers it.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

#[cfg(feature = "cranelift")]
const ENGINES: &[&str] = &["--vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES: &[&str] = &["--vm"];

fn run(engine: &str, src: &str, argv: &[&str]) -> (bool, String, String) {
    let mut cmd = ilo();
    cmd.arg(engine).arg(src);
    for a in argv {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to spawn ilo");
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    (out.status.success(), stdout, stderr)
}

// ── jpth on a JSON array field: typed list, len works directly ─────────

#[test]
fn jpth_array_field_returns_iterable_list() {
    // The event-trace-analyser repro: `{"spans":[{...},{...}]}`. Before the
    // fix `jpth blob "spans"` returned a text blob and `len ss` raised a
    // type error. After the fix the Ok variant is `L _`, so `len` reports
    // element count directly. We `?r{~v:..;^e:..}`-match to satisfy the
    // non-Result-returning main; the test is on the happy path so the Err
    // arm only ever fires on regression.
    let json = r#"{"spans":[{"id":"a"},{"id":"b"},{"id":"c"}]}"#;
    let src = r#"main j:t>n;r=jpth j "spans";?r{~v:len v;^e:0}"#;
    for engine in ENGINES {
        let (ok, stdout, stderr) = run(engine, src, &[json]);
        assert!(
            ok,
            "engine {engine} failed: stdout={stdout:?} stderr={stderr:?}"
        );
        assert_eq!(
            stdout, "3",
            "engine {engine}: expected len=3, got stdout={stdout:?}"
        );
    }
}

// ── jpth on a JSON object field: typed value, jdmp round-trips it ──────

#[test]
fn jpth_object_field_returns_typed_value() {
    // The monorepo-analyst repro: `{"dependencies":{"a":"1","b":"2"}}`. Before
    // the fix `jpth pkg "dependencies"` returned the stringified object and
    // anything that wanted a structured value (mkeys, jkeys, field-access)
    // rejected it. After the fix the Ok variant is a record; jdmp round-trips
    // and we assert both keys are present (avoiding sort-order assumptions).
    let json = r#"{"dependencies":{"a":"1","b":"2"}}"#;
    let src = r#"main j:t>t;r=jpth j "dependencies";?r{~v:jdmp v;^e:e}"#;
    for engine in ENGINES {
        let (ok, stdout, stderr) = run(engine, src, &[json]);
        assert!(
            ok,
            "engine {engine} failed: stdout={stdout:?} stderr={stderr:?}"
        );
        assert!(
            stdout.contains("\"a\"") && stdout.contains("\"b\""),
            "engine {engine}: expected re-jdmp'd object to contain a + b keys, got stdout={stdout:?}"
        );
    }
}

// ── jpth scalar leaves: numbers come back as Number, not Text ──────────

#[test]
fn jpth_numeric_leaf_returns_number() {
    // Pre-fix `jpth blob "n"` on `{"n":42}` returned Text("42"). Post-fix it
    // returns Number(42), so arithmetic works directly without `num!`.
    let json = r#"{"n":42}"#;
    let src = r#"main j:t>n;r=jpth j "n";?r{~v:+ v 1;^e:0}"#;
    for engine in ENGINES {
        let (ok, stdout, stderr) = run(engine, src, &[json]);
        assert!(
            ok,
            "engine {engine} failed: stdout={stdout:?} stderr={stderr:?}"
        );
        assert_eq!(
            stdout, "43",
            "engine {engine}: expected n+1=43, got stdout={stdout:?}"
        );
    }
}

// ── jkeys: sorted top-level keys of object at path ─────────────────────

#[test]
fn jkeys_returns_sorted_top_level_keys() {
    // jkeys is the explicit "I just want the keys" companion to mkeys. The
    // monorepo-analyst persona needed this to enumerate package dependencies
    // without shelling out to jq. Sorted output is intentional so cross-engine
    // diffs stay deterministic.
    let json = r#"{"deps":{"left-pad":"1.0","react":"18","axios":"1.6"}}"#;
    let src = r#"main j:t>t;r=jkeys j "deps";?r{~v:jdmp v;^e:e}"#;
    for engine in ENGINES {
        let (ok, stdout, stderr) = run(engine, src, &[json]);
        assert!(
            ok,
            "engine {engine} failed: stdout={stdout:?} stderr={stderr:?}"
        );
        // Sorted: axios, left-pad, react. Asserting the literal JSON output
        // pins both the sort and the typed-list-of-text shape.
        assert_eq!(
            stdout, r#"["axios","left-pad","react"]"#,
            "engine {engine}: expected sorted [axios,left-pad,react], got stdout={stdout:?}"
        );
    }
}

#[test]
fn jkeys_on_root_with_empty_path() {
    // Empty path == top-level keys. Mirrors `mkeys` over a freshly-parsed map.
    let json = r#"{"z":1,"a":2,"m":3}"#;
    let src = r#"main j:t>t;r=jkeys j "";?r{~v:jdmp v;^e:e}"#;
    for engine in ENGINES {
        let (ok, stdout, stderr) = run(engine, src, &[json]);
        assert!(
            ok,
            "engine {engine} failed: stdout={stdout:?} stderr={stderr:?}"
        );
        assert_eq!(
            stdout, r#"["a","m","z"]"#,
            "engine {engine}: expected sorted [a,m,z], got stdout={stdout:?}"
        );
    }
}

#[test]
fn jkeys_on_non_object_returns_err() {
    // Calling jkeys on an array or scalar surfaces an Err so the caller can
    // branch instead of silently getting an empty list and assuming a
    // key-less object existed. We use the `?r{~v:..;^e:..}` Result match to
    // force the Err arm and emit a string the test can grep for.
    let json = r#"{"xs":[1,2,3]}"#;
    let src = r#"main j:t>t;r=jkeys j "xs";?r{~v:jdmp v;^e:cat ["ERR:",e] ""}"#;
    for engine in ENGINES {
        let (ok, stdout, stderr) = run(engine, src, &[json]);
        assert!(
            ok,
            "engine {engine} failed: stdout={stdout:?} stderr={stderr:?}"
        );
        assert!(
            stdout.contains("ERR:") && stdout.to_lowercase().contains("object"),
            "engine {engine}: expected Err mentioning 'object', got stdout={stdout:?}"
        );
    }
}
