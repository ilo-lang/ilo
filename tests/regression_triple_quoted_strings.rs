// Cross-engine regression tests for triple-quoted multi-line string
// literals (#34).
//
// Before: agents had to assemble multi-line content with `cat` and
// embedded `\n` escapes, which is verbose and easy to get wrong. The
// triple-quoted form (`"""..."""`) lets the body carry raw newlines and
// applies `indoc!`-style indent stripping when the closing delimiter
// sits on its own line, matching Python / Rust intuition.
//
// These tests pin behaviour across tree, VM, and Cranelift JIT so a
// backend drift can't silently re-break the surface. They cover:
//
//   - single-line `"""x"""` (same shape as `"x"`)
//   - multi-line with embedded newlines preserved
//   - Python/PEP-257-style leading-newline drop + common-indent strip
//     when the closing `"""` sits on its own line
//   - terminating newline of the last content line is preserved
//   - escape sequences (`\n`, `\t`, ...) still decode inside `"""..."""`
//   - `{name}` interpolation works identically to single-quoted strings
//   - empty body `""""""` is the empty string
//
// The lexer implements this in `normalize_newlines_with_map` by
// scanning for the closing `"""` and emitting a synthesised single-
// quoted form, so logos' existing string regex consumes it. Span
// attribution maps every emitted byte back to its original source byte
// for diagnostic accuracy.

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
        "ilo {engine} src failed:\n  src = {src:?}\n  stderr = {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .trim_end_matches('\n')
        .to_string()
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--vm"];

/// Single-line triple-quoted form behaves exactly like a regular
/// double-quoted string. No newline, no indent stripping.
#[test]
fn triple_single_line_cross_engine() {
    let src = r#"f>n;len """hello""""#;
    for engine in ENGINES_ALL {
        let out = run_noarg(engine, src, "f");
        assert_eq!(out, "5", "{engine}: single-line triple quoted");
    }
}

/// Inline triple-quoted with embedded newline: opening + closing on the
/// same lines as content, no indent stripping (PEP 257 only kicks in
/// when the closing `"""` is on its own line).
#[test]
fn triple_inline_newline_cross_engine() {
    let src = "f>n;len \"\"\"foo\nbar\"\"\"";
    // "foo" + "\n" + "bar" = 7 chars
    for engine in ENGINES_ALL {
        let out = run_noarg(engine, src, "f");
        assert_eq!(out, "7", "{engine}: inline newline");
    }
}

/// Indent stripping: leading newline dropped, common indent (matching
/// the closing-`"""`-line indent) removed from each content line.
/// Terminating `\n` of the last content line is preserved.
#[test]
fn triple_dedent_cross_engine() {
    let src = "f>n\n  len \"\"\"\n  hello\n  world\n  \"\"\"";
    // After strip: "hello\nworld\n" = 5 + 1 + 5 + 1 = 12 chars
    for engine in ENGINES_ALL {
        let out = run_noarg(engine, src, "f");
        assert_eq!(out, "12", "{engine}: dedented multi-line");
    }
}

/// Returned text content matches "hello\nworld\n" exactly after dedent.
#[test]
fn triple_dedent_content_cross_engine() {
    let src = "f>t\n  \"\"\"\n  hello\n  world\n  \"\"\"";
    for engine in ENGINES_ALL {
        let out = ilo()
            .args([src, engine, "f"])
            .output()
            .expect("failed to run ilo");
        assert!(
            out.status.success(),
            "{engine}: stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        // `ilo` itself prints text values verbatim followed by a final
        // newline of its own. Strip that single trailing print-newline
        // to compare the string value the program produced.
        let stdout = String::from_utf8_lossy(&out.stdout);
        let s = stdout.strip_suffix('\n').unwrap_or(&stdout);
        assert_eq!(s, "hello\nworld\n", "{engine}: dedent content");
    }
}

/// `{name}` interpolation works identically inside `"""..."""` as it
/// does inside `"..."`. Both lower onto the same `fmt` desugaring.
#[test]
fn triple_interpolation_cross_engine() {
    let src = "f>t\n  name=\"dan\"\n  \"\"\"hi {name}\"\"\"";
    for engine in ENGINES_ALL {
        let out = ilo()
            .args([src, engine, "f"])
            .output()
            .expect("failed to run ilo");
        assert!(
            out.status.success(),
            "{engine}: stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        let s = stdout.strip_suffix('\n').unwrap_or(&stdout);
        assert_eq!(s, "hi dan", "{engine}: triple-quoted interpolation");
    }
}

/// Multi-line content with interpolation slots spread across lines.
#[test]
fn triple_interp_multiline_cross_engine() {
    let src = "f>t\n  a=\"x\"\n  b=\"y\"\n  \"\"\"\n  {a}\n  {b}\n  \"\"\"";
    for engine in ENGINES_ALL {
        let out = ilo()
            .args([src, engine, "f"])
            .output()
            .expect("failed to run ilo");
        assert!(
            out.status.success(),
            "{engine}: stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        let s = stdout.strip_suffix('\n').unwrap_or(&stdout);
        assert_eq!(s, "x\ny\n", "{engine}: multi-line interp");
    }
}

/// Escape sequences inside triple-quoted strings decode exactly as they
/// do inside double-quoted strings. This is what makes the form a
/// drop-in upgrade rather than a parallel surface.
#[test]
fn triple_escapes_cross_engine() {
    let src = r#"f>n;len """a\nb""""#;
    for engine in ENGINES_ALL {
        let out = run_noarg(engine, src, "f");
        // `\n` decodes to one byte → "a" + LF + "b" = 3 chars.
        assert_eq!(out, "3", "{engine}: escapes decode");
    }
}

/// Empty triple-quoted body is the empty string.
#[test]
fn triple_empty_cross_engine() {
    let src = r#"f>n;len """""""#;
    for engine in ENGINES_ALL {
        let out = run_noarg(engine, src, "f");
        assert_eq!(out, "0", "{engine}: empty triple-quoted");
    }
}

/// Embedded single `"` inside a triple-quoted body is treated literally
/// (only `"""` ends the literal). The decoded string carries the bare
/// quote.
#[test]
fn triple_embedded_single_quote_cross_engine() {
    let src = "f>n;len \"\"\"a\"b\"\"\"";
    for engine in ENGINES_ALL {
        let out = run_noarg(engine, src, "f");
        // "a" + "\"" + "b" = 3 chars
        assert_eq!(out, "3", "{engine}: embedded single quote");
    }
}
