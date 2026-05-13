// Cross-engine regression tests for the `chars` builtin.
//
// `chars s` explodes a string into a list of single-char strings, one per
// Unicode scalar. Unlike `spl s ""`, it does NOT emit sentinel empty strings
// at either end — empty input yields an empty list, not `[""]`.
//
// All three engines (tree, VM, Cranelift) must agree on every case.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str, arg: &str) -> String {
    let out = ilo()
        .args([src, engine, entry, arg])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} {src:?} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_noarg(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} {src:?} failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm"];

const CHARS_SRC: &str = "f s:t>L t;chars s";
const CHARS_LEN_SRC: &str = "f s:t>n;len (chars s)";
// Inline literals so we can exercise empty input without arg-parsing quirks.
const CHARS_EMPTY_SRC: &str = "f>L t;chars \"\"";
const CHARS_UNICODE_SRC: &str = "f>L t;chars \"café\"";
const CHARS_EMOJI_SRC: &str = "f>L t;chars \"a😀b\"";

#[test]
fn chars_basic_ascii_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, CHARS_SRC, "f", "abc");
        assert_eq!(out, "[a, b, c]", "{engine}: chars basic ascii");
    }
}

#[test]
fn chars_empty_returns_empty_list_cross_engine() {
    // The whole point of this builtin: empty input must yield an empty
    // list, NOT a list containing one empty string. This is the bug that
    // contaminates AA-frequency maps when callers reach for `spl s ""`.
    for engine in ENGINES_ALL {
        let out = run_noarg(engine, CHARS_EMPTY_SRC, "f");
        assert_eq!(out, "[]", "{engine}: chars empty must return []");
    }
}

#[test]
fn chars_no_sentinel_strings_cross_engine() {
    // Length should equal the number of scalars, never N+2 (no leading or
    // trailing sentinel empty strings).
    for engine in ENGINES_ALL {
        let out = run(engine, CHARS_LEN_SRC, "f", "hello");
        assert_eq!(out, "5", "{engine}: chars len = scalar count");
    }
}

#[test]
fn chars_unicode_cross_engine() {
    // `é` is a single scalar (U+00E9) — must come out as one element,
    // not two bytes.
    for engine in ENGINES_ALL {
        let out = run_noarg(engine, CHARS_UNICODE_SRC, "f");
        assert_eq!(out, "[c, a, f, é]", "{engine}: chars unicode scalar");
    }
}

#[test]
fn chars_emoji_cross_engine() {
    // Astral-plane scalar (U+1F600) — must come out as one element.
    for engine in ENGINES_ALL {
        let out = run_noarg(engine, CHARS_EMOJI_SRC, "f");
        assert_eq!(out, "[a, 😀, b]", "{engine}: chars emoji scalar");
    }
}

#[test]
fn chars_single_char_cross_engine() {
    for engine in ENGINES_ALL {
        let out = run(engine, CHARS_SRC, "f", "x");
        assert_eq!(out, "[x]", "{engine}: chars single char");
    }
}
