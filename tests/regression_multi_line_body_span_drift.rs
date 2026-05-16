//! Regression: parse-error spans inside multi-line function bodies used
//! to drift onto a downstream `;` (often 3-20 lines past the real fault)
//! because the lexer scans `normalize_newlines(source)` and emits spans
//! in *normalized* coordinates, while the diagnostic layer resolves them
//! against the *original* source via `SourceMap`.
//!
//! Each rewritten character (newline → `;`, stripped indent, dropped
//! comment text) compounds the drift. Personas flagged this every rerun
//! once multi-line bodies became viable; quant-trader rerun3 and
//! security-researcher rerun2 each lost ~5min bisecting the wrong line.
//!
//! Fix: `lexer::normalize_newlines_with_map` returns a byte-level
//! original-offset map alongside the normalized string. `lex` remaps
//! every token span (and lex-error position) back to original-source
//! coordinates before returning, so `SourceMap::lookup` and downstream
//! span-consumers see offsets that match what the user typed.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Run `ilo --json <file>` and return stderr. JSON mode gives an
/// unambiguous `line` field per label.
fn run_err_json_file(path: &str) -> String {
    let out = ilo()
        .arg("--json")
        .arg(path)
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected failure for {path:?}, stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn write_tmp(name: &str, src: &str) -> String {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("ilo-multiline-span-{name}.ilo"));
    std::fs::write(&path, src).expect("write tmp file");
    path.to_string_lossy().into_owned()
}

/// Find the primary error label's `line` value. The primary label is
/// always emitted first by the JSON renderer, so we just grab the first
/// `"line":N` we see.
fn first_error_line(stderr: &str) -> usize {
    let key = "\"line\":";
    let idx = stderr
        .find(key)
        .unwrap_or_else(|| panic!("no line field in stderr:\n{stderr}"));
    let tail = &stderr[idx + key.len()..];
    let end = tail
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(tail.len());
    tail[..end]
        .parse()
        .unwrap_or_else(|_| panic!("could not parse line number from stderr:\n{stderr}"))
}

/// Like `first_error_line` but also returns the primary `start` byte
/// offset, used to assert the span sits on the offending token itself
/// rather than a downstream `;` separator.
fn first_error_line_and_start(stderr: &str) -> (usize, usize) {
    let line = first_error_line(stderr);
    let start_key = "\"start\":";
    let s_idx = stderr
        .find(start_key)
        .unwrap_or_else(|| panic!("no start field in stderr:\n{stderr}"));
    let tail = &stderr[s_idx + start_key.len()..];
    let end = tail
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(tail.len());
    let start: usize = tail[..end]
        .parse()
        .unwrap_or_else(|_| panic!("could not parse start from stderr:\n{stderr}"));
    (line, start)
}

#[test]
fn rev_binding_in_indented_main_body_lands_on_actual_line() {
    // Canonical persona repro: `rev = ...` inside a multi-line main body.
    // Before the fix the ILO-P011 span anchored to line 5 (the first body
    // statement after the header) regardless of how far down `rev =` lives.
    let src = "helper p:_>R n t;\n  ~mget!! m p\n\nmain>R t t\n  s = \"x\"\n  a = 1\n  b = 2\n  c = 3\n  rev = mget!! m p\n  ~s\n";
    let path = write_tmp("rev-in-body", src);
    let err = run_err_json_file(&path);
    let (line, start) = first_error_line_and_start(&err);
    assert_eq!(
        line, 9,
        "ILO-P011 must point at the `rev =` line (9), got stderr:\n{err}"
    );
    // The span's start byte must sit inside the `rev` token in the
    // original source — not on a `;` upstream.
    let rev_off = src.find("rev =").expect("repro contains `rev =`");
    assert_eq!(
        start, rev_off,
        "ILO-P011 start byte must be the `r` of `rev` ({rev_off}), got {start}, stderr:\n{err}"
    );
}

#[test]
fn foreach_body_parse_error_lands_inside_loop() {
    // Parse error inside a `@i 0..n{...}` body. Before the fix the
    // normalize_newlines `;` rewrites collapsed every body statement onto
    // a single line and the error landed on the closing `;` of the loop.
    let src = "main>n\n  n = 5\n  acc = 0\n  @i 0..n{\n    bad = wibble\n  }\n  ~acc\n";
    let path = write_tmp("foreach", src);
    let err = run_err_json_file(&path);
    let line = first_error_line(&err);
    assert_eq!(
        line, 5,
        "error must land on the `bad = wibble` line (5), got stderr:\n{err}"
    );
}

#[test]
fn guard_body_parse_error_does_not_drift_to_next_arm() {
    // Multi-statement guard body. The fault is on the `wat =` line, not
    // on the `~"err"` continuation. Drift used to land it on the closing
    // `};` instead.
    let src = "main>R t t\n  s = \"abc\"\n  =s \"abc\"{\n    a = 1\n    wat = bogus\n    ~\"ok\"\n  }\n  ~s\n";
    let path = write_tmp("guard", src);
    let err = run_err_json_file(&path);
    let line = first_error_line(&err);
    assert_eq!(
        line, 5,
        "error must land on `wat = bogus` (5), got stderr:\n{err}"
    );
}

#[test]
fn match_arm_body_parse_error_lands_on_offending_token() {
    // Deeply indented match-arm brace body with the fault several lines
    // in. Match-arm bodies (`pat:{...}`) compose with the same body-line
    // normalization as the rest of multi-line syntax, so the offset map
    // has to thread through here too. Without it, the ILO-P011 span
    // drifted forward to a downstream arm separator.
    let src = "main>n\n  r = num \"1\"\n  y = ?r{\n    ~v:{\n      a = 2\n      rev = +a v\n      *a 3\n    }\n    ^e:0\n  }\n  y\n";
    let path = write_tmp("match-arm", src);
    let err = run_err_json_file(&path);
    let line = first_error_line(&err);
    assert_eq!(
        line, 6,
        "ILO-P011 must land on `rev = +a v` (6), got stderr:\n{err}"
    );
}

#[test]
fn deeply_nested_body_span_does_not_drift() {
    // Two levels of nesting (foreach inside guard inside main) puts many
    // `;` rewrites between the start of main and the faulting binding.
    // Before the fix the span drifted by 4+ lines.
    let src = "main>n\n  n = 3\n  acc = 0\n  >n 0{\n    @i 0..n{\n      t = +acc i\n      rev = t\n      acc = +acc 1\n    }\n  }\n  ~acc\n";
    let path = write_tmp("nested", src);
    let err = run_err_json_file(&path);
    let line = first_error_line(&err);
    assert_eq!(
        line, 7,
        "ILO-P011 must land on `rev = t` (7), got stderr:\n{err}"
    );
}

#[test]
fn function_last_statement_parse_error_lands_on_last_line() {
    // Fault on the final statement of a long multi-line body. Drift
    // historically pushed the span back to an earlier statement because
    // each preceding line shed indent and gained a `;`.
    let src = "main>n\n  a = 1\n  b = 2\n  c = 3\n  d = 4\n  e = 5\n  rev = 6\n";
    let path = write_tmp("last-stmt", src);
    let err = run_err_json_file(&path);
    let (line, start) = first_error_line_and_start(&err);
    assert_eq!(
        line, 7,
        "ILO-P011 must land on the last `rev = 6` line (7), got stderr:\n{err}"
    );
    let rev_off = src.find("rev = 6").expect("repro contains `rev = 6`");
    assert_eq!(
        start, rev_off,
        "span start ({start}) must equal byte offset of `rev` ({rev_off}), stderr:\n{err}"
    );
}

#[test]
fn comment_stripping_does_not_shift_following_line_span() {
    // `normalize_newlines` drops `--` comment text entirely before
    // emitting `;`/`\n`. Without the offset map the bytes after the
    // comment line shift backward by `comment.len()`, so a fault on the
    // very next line landed at column 0 of a phantom earlier offset.
    let src = "main>n\n  a = 1\n  -- explanatory comment text that is long\n  rev = 2\n  ~a\n";
    let path = write_tmp("comment", src);
    let err = run_err_json_file(&path);
    let (line, start) = first_error_line_and_start(&err);
    assert_eq!(
        line, 4,
        "ILO-P011 must land on `rev = 2` (4), got stderr:\n{err}"
    );
    let rev_off = src.find("rev = 2").expect("repro contains `rev = 2`");
    assert_eq!(
        start, rev_off,
        "span start must equal `r` byte, stderr:\n{err}"
    );
}

#[test]
fn single_line_body_span_unchanged() {
    // Sanity: when no newline rewriting happens, spans must stay
    // identical to pre-fix behaviour. This pins the no-drift case so a
    // future refactor that breaks the identity branch surfaces here.
    let src = "main>n;rev = 1\n";
    let path = write_tmp("single-line", src);
    let err = run_err_json_file(&path);
    let (line, start) = first_error_line_and_start(&err);
    assert_eq!(line, 1, "single-line fault on line 1, got stderr:\n{err}");
    let rev_off = src.find("rev = 1").expect("repro contains `rev = 1`");
    assert_eq!(
        start, rev_off,
        "span start must equal `r` byte, stderr:\n{err}"
    );
}
