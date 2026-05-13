// Regression tests for `MapKey::Int` numeric map keys.
//
// Before MapKey, `Value::Map` was `HashMap<String, Value>` — every iteration
// keyed by a loop index had to run `k=str j` first (the "routing-tsp tax").
// With MapKey, numeric keys are first-class via `MapKey::Int(i64)`. Floats
// floor to i64 at the builtin boundary (matching `at xs i`); NaN/Infinity are
// rejected as runtime errors.
//
// These tests run the typed-key surface (mset / mget / mhas / mkeys / mvals /
// mdel / len / jdmp / iteration determinism) through every engine (tree, VM,
// cranelift) to catch backend drift. Text-key coverage stays in place to
// guard against regressions in the existing behaviour.

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
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

const ENGINES: &[&str] = &[
    "--run-tree",
    "--run-vm",
    #[cfg(feature = "cranelift")]
    "--run-cranelift",
];

// ── Text keys: existing behaviour stays correct ──────────────────────────

#[test]
fn text_key_mget() {
    let src = r#"f>n;m=mmap;m=mset m "a" 7;r=mget m "a";?r{n v:v;_:-1}"#;
    for e in ENGINES {
        assert_eq!(run(e, src, "f"), "7", "engine={e}");
    }
}

#[test]
fn text_key_mhas() {
    let src = r#"f>b;m=mmap;m=mset m "k" 1;mhas m "k""#;
    for e in ENGINES {
        assert_eq!(run(e, src, "f"), "true", "engine={e}");
    }
}

#[test]
fn text_key_mdel_then_miss() {
    let src = r#"f>n;m=mmap;m=mset m "k" 1;m=mdel m "k";r=mget m "k";?r{n v:v;_:0}"#;
    for e in ENGINES {
        assert_eq!(run(e, src, "f"), "0", "engine={e}");
    }
}

// ── Int keys: the new behaviour ──────────────────────────────────────────

#[test]
fn int_key_mset_mget() {
    let src = r#"f>n;m=mmap;m=mset m 7 100;r=mget m 7;?r{n v:v;_:-1}"#;
    for e in ENGINES {
        assert_eq!(run(e, src, "f"), "100", "engine={e}");
    }
}

#[test]
fn int_key_mhas_present() {
    let src = r#"f>b;m=mmap;m=mset m 42 0;mhas m 42"#;
    for e in ENGINES {
        assert_eq!(run(e, src, "f"), "true", "engine={e}");
    }
}

#[test]
fn int_key_mhas_missing() {
    let src = r#"f>b;m=mmap;m=mset m 42 0;mhas m 99"#;
    for e in ENGINES {
        assert_eq!(run(e, src, "f"), "false", "engine={e}");
    }
}

#[test]
fn int_key_mdel_removes_entry() {
    let src = r#"f>n;m=mmap;m=mset m 1 10;m=mset m 2 20;m=mdel m 1;len (mkeys m)"#;
    for e in ENGINES {
        assert_eq!(run(e, src, "f"), "1", "engine={e}");
    }
}

#[test]
fn int_key_mget_missing_returns_nil() {
    let src = r#"f>n;m=mmap;m=mset m 1 99;r=mget m 2;?r{n v:v;_:-1}"#;
    for e in ENGINES {
        assert_eq!(run(e, src, "f"), "-1", "engine={e}");
    }
}

#[test]
fn int_key_mkeys_sorted_deterministic() {
    // mkeys order is non-deterministic; sort it to get a stable answer.
    // hd (srt ks) must be the smallest key regardless of engine.
    let src = r#"f>n;m=mmap;m=mset m 3 0;m=mset m 1 0;m=mset m 2 0;ks=srt (mkeys m);hd ks"#;
    for e in ENGINES {
        assert_eq!(run(e, src, "f"), "1", "engine={e}");
    }
}

#[test]
fn int_key_mvals_count() {
    let src = r#"f>n;m=mmap;m=mset m 1 10;m=mset m 2 20;m=mset m 3 30;len (mvals m)"#;
    for e in ENGINES {
        assert_eq!(run(e, src, "f"), "3", "engine={e}");
    }
}

#[test]
fn int_key_len_via_mkeys() {
    let src = r#"f>n;m=mmap;m=mset m 10 0;m=mset m 20 0;m=mset m 30 0;len (mkeys m)"#;
    for e in ENGINES {
        assert_eq!(run(e, src, "f"), "3", "engine={e}");
    }
}

// ── Int vs Text: distinct keys, no collision ────────────────────────────

#[test]
fn int_key_distinct_from_text_key_via_frq() {
    // frq [1, "1"] gives Int(1)=1 and Text("1")=1 — two slots, count 1 each.
    let src_num = r#"f>n;m=frq [1, "1"];r=mget m 1;?r{n v:v;_:-1}"#;
    let src_txt = r#"f>n;m=frq [1, "1"];r=mget m "1";?r{n v:v;_:-1}"#;
    for e in ENGINES {
        assert_eq!(run(e, src_num, "f"), "1", "engine={e} numeric key");
        assert_eq!(run(e, src_txt, "f"), "1", "engine={e} text key");
    }
}

// ── Float keys floor to i64 at the builtin boundary ──────────────────────

#[test]
fn float_key_floors_to_int() {
    // 0.7 floors to 0; mget with 0 should find it.
    let src = r#"f>n;m=mmap;m=mset m 0.7 42;r=mget m 0;?r{n v:v;_:-1}"#;
    for e in ENGINES {
        assert_eq!(run(e, src, "f"), "42", "engine={e}");
    }
}

// ── jdmp: numeric keys serialise as JSON string keys ─────────────────────

#[test]
fn jdmp_int_key_stringifies() {
    // JSON object keys must be strings, so MapKey::Int(7) serialises as "7".
    let src = r#"f>t;m=mmap;m=mset m 7 42;jdmp m"#;
    for e in ENGINES {
        let out = run(e, src, "f");
        assert!(
            out.contains("\"7\":42"),
            "engine={e} expected key \"7\":42, got: {out}"
        );
    }
}

// ── Iteration determinism: srt (mkeys m) is stable across engines ───────

#[test]
fn int_key_iteration_deterministic_across_engines() {
    // Insert in a scrambled order; sort the resulting key list and check
    // every engine agrees on the canonical order.
    let src = r#"f>t;m=mmap;m=mset m 5 0;m=mset m 1 0;m=mset m 3 0;m=mset m 2 0;m=mset m 4 0;ks=srt (mkeys m);fmt "{},{},{},{},{}" (str (at ks 0)) (str (at ks 1)) (str (at ks 2)) (str (at ks 3)) (str (at ks 4))"#;
    for e in ENGINES {
        assert_eq!(run(e, src, "f"), "1,2,3,4,5", "engine={e}");
    }
}
