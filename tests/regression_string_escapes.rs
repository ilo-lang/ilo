// Cross-engine regression tests for string-literal escape sequences.
//
// Root cause investigated: the lexer's string-literal callback only mapped
// `\n \t \r \" \\`. Everything else (notably `\f`, the form-feed character
// `pdftotext` writes between PDF pages) fell through to the "unknown
// escape" branch and was emitted as literal backslash + letter. The
// pdf-analyst persona on the Llama 3 paper rerun hit this on `spl raw
// "\f"` returning 1 over a 92-page PDF.
//
// Fix: the lexer now decodes the standard C/JSON escape set —
// `\n \t \r \" \\ \f \b \v \a \0 \/` — matching what every other modern
// language does and what pdftotext / ANSI / null-terminated formats need.
//
// These tests pin behaviour across tree, VM, and Cranelift so a backend
// drift can't silently re-break it. We exercise `spl` (the original
// pdf-analyst use case) plus `len` of the decoded string so we catch the
// "looks right but is two chars" failure mode the lexer used to produce.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
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

/// The original pdf-analyst case: split a multi-page document on form-feed.
/// Before the fix every engine returned 1 (because `"\f"` was two chars,
/// not the 0x0C separator pdftotext emits). After the fix every engine
/// returns 3 — one element per page plus the empty tail.
#[test]
fn formfeed_split_cross_engine() {
    let src = "f>n;len (spl \"page1\u{000C}page2\u{000C}page3\" \"\\f\")";
    for engine in ENGINES_ALL {
        let out = run_noarg(engine, src, "f");
        assert_eq!(out, "3", "{engine}: spl on form-feed");
    }
}

/// Each single-char escape must decode to exactly one Unicode scalar, not
/// the literal backslash-letter pair. `len` is the bluntest detector for
/// the "unknown escape" fallthrough bug.
#[test]
fn single_escape_len_one_cross_engine() {
    let escapes = [
        ("\\n", "newline"),
        ("\\t", "tab"),
        ("\\r", "carriage return"),
        ("\\f", "form feed"),
        ("\\b", "backspace"),
        ("\\v", "vertical tab"),
        ("\\a", "bell"),
        ("\\0", "null"),
        ("\\/", "forward slash"),
        ("\\\\", "backslash"),
        ("\\\"", "double quote"),
    ];
    for (esc, name) in escapes {
        let src = format!("f>n;len \"{esc}\"");
        for engine in ENGINES_ALL {
            let out = run_noarg(engine, &src, "f");
            assert_eq!(out, "1", "{engine}: len of {name} ({esc}) should be 1");
        }
    }
}

/// Form-feed decoded value must be 0x0C across engines (not 92, which is
/// the `\` codepoint, the most likely failure mode under a partial fix).
#[test]
fn formfeed_codepoint_cross_engine() {
    // `at s 0` yields a single-char string for the 0th scalar; we then
    // chain into `chars` + `len` to keep the test purely string-shaped.
    // For a hard codepoint check, lean on the lexer unit test in
    // src/lexer/mod.rs — this case proves the runtime sees one scalar.
    let src = "f>n;len (chars \"\\f\")";
    for engine in ENGINES_ALL {
        let out = run_noarg(engine, src, "f");
        assert_eq!(out, "1", "{engine}: form-feed is one scalar");
    }
}

/// Mixed escape sequences in one literal must all decode. This catches a
/// bug where (e.g.) only the first escape on a line gets processed.
#[test]
fn mixed_escapes_cross_engine() {
    // Six escapes, six characters.
    let src = "f>n;len \"\\n\\t\\f\\r\\b\\0\"";
    for engine in ENGINES_ALL {
        let out = run_noarg(engine, src, "f");
        assert_eq!(out, "6", "{engine}: mixed escapes");
    }
}

/// Unknown escapes (e.g. `\z`) keep the pre-fix passthrough behaviour:
/// the literal stays two chars (`\` + `z`). Locking this in so future
/// additions to the escape table don't silently flip behaviour for
/// strings that already rely on the fallback.
#[test]
fn unknown_escape_preserves_backslash_cross_engine() {
    let src = "f>n;len \"\\z\"";
    for engine in ENGINES_ALL {
        let out = run_noarg(engine, src, "f");
        assert_eq!(out, "2", "{engine}: unknown escape passes through");
    }
}
