//! Regression: parse-error spans for a malformed function header used to
//! land on the WRONG function in multi-function files. The parser filters
//! all `Token::Newline`s globally, so when the header of function N is
//! malformed (e.g. `f2 a:n>R` missing the err-type, or `f2 a:n` missing
//! `>type;body`), `parse_type` / `parse_params` happily walked across the
//! newline and consumed tokens from function N+1. The resulting error span
//! pointed at function N+1, sending personas to bisect the wrong line.
//!
//! Fix: `Parser::new` now records top-level decl boundaries (unindented
//! newlines, which `lexer::normalize_newlines` already preserves as
//! `Token::Newline`) before filtering them out. `parse_fn_decl` checks the
//! boundary between params/`>`/return-type and emits the friendly ILO-P020
//! anchored at the offending function's name. `parse_params` stops at a
//! boundary so it can't slurp the next function's name as another param.
//! `parse_type` has a safety-net check for nested type slots
//! (`R`/`M`/`F`/`L`/`O`/`S`) that anchors its ILO-P007 at the previous
//! token's line instead of the next decl.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Run `ilo --json <src>` and return stderr. JSON diagnostic mode makes
/// the `line` field unambiguous, which is what these tests pin on.
fn run_err_json(src: &str) -> String {
    let out = ilo()
        .arg("--json")
        .arg(src)
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected failure for {src:?}, stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn run_ok(src: &str) {
    let out = ilo().arg(src).output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "expected success for {src:?}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Find the first `line` value in JSON diagnostic stderr. Crude but enough
/// for these tests since the first error is always the one we care about.
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

fn first_error_code(stderr: &str) -> String {
    let key = "\"code\":\"";
    let idx = stderr
        .find(key)
        .unwrap_or_else(|| panic!("no code field in stderr:\n{stderr}"));
    let tail = &stderr[idx + key.len()..];
    let end = tail.find('"').expect("unterminated code field");
    tail[..end].to_string()
}

#[test]
fn missing_err_type_attributes_to_offending_function() {
    // `f2`'s header has `>R` with no err-type before the line ends. The
    // error must land on line 2 (where `f2` lives), not on line 3 (`main`)
    // where the parser used to land it after walking across the newline.
    let src = "f1 a:n>n;+a 1\nf2 a:n>R\nmain>n;0";
    let err = run_err_json(src);
    assert_eq!(
        first_error_line(&err),
        2,
        "expected error on line 2 (f2), got stderr:\n{err}"
    );
    // The safety net inside `parse_type` fires here with the new "got end
    // of line" wording.
    assert!(
        err.contains("end of line"),
        "expected 'end of line' wording in stderr:\n{err}"
    );
}

#[test]
fn missing_arrow_attributes_to_offending_function() {
    // `f2 a:n` has no `>type;body`. The parser used to keep `parse_params`
    // running, slurp `main` as the next param, then surface a P003 on
    // line 3. Now ILO-P020 fires anchored at `f2` on line 2.
    let src = "f1 a:n>n;+a 1\nf2 a:n\nmain>n;0";
    let err = run_err_json(src);
    assert_eq!(
        first_error_line(&err),
        2,
        "expected error on line 2 (f2), got stderr:\n{err}"
    );
    assert_eq!(
        first_error_code(&err),
        "ILO-P020",
        "expected ILO-P020 for incomplete header, got stderr:\n{err}"
    );
    assert!(
        err.contains("`f2`"),
        "expected error to name `f2`, got stderr:\n{err}"
    );
}

#[test]
fn missing_return_type_after_arrow_attributes_to_offending_function() {
    // `f2 a:n>` ends with `>` and no return type. The space between `>`
    // and the newline is where the header gives up.
    let src = "f1 a:n>n;+a 1\nf2 a:n>\nmain>n;0";
    let err = run_err_json(src);
    assert_eq!(
        first_error_line(&err),
        2,
        "expected error on line 2 (f2), got stderr:\n{err}"
    );
    // Either ILO-P020 (boundary check between `>` and return type) or
    // ILO-P007 (parse_type safety net), both anchored at line 2.
    let code = first_error_code(&err);
    assert!(
        code == "ILO-P020" || code == "ILO-P007",
        "expected ILO-P020 or ILO-P007, got {code}, stderr:\n{err}"
    );
}

#[test]
fn error_on_middle_of_three_functions_does_not_bleed_either_way() {
    // Three-function file with the fault in the middle. The error must
    // land on line 2 (the offending function) — not line 1 (the prior
    // function) and not line 3 (the next function). This is the strongest
    // shape of the regression: bleed in either direction is a bug.
    let src = "f1 a:n>n;+a 1\nf2 a:n>R\nf3 a:n>n;-a 1";
    let err = run_err_json(src);
    assert_eq!(
        first_error_line(&err),
        2,
        "error must land on line 2 (f2), got stderr:\n{err}"
    );
}

#[test]
fn error_on_first_function_stays_on_first_function() {
    // Symmetric: fault on line 1. Even without a previous function to
    // bleed onto, the span must stay on line 1.
    let src = "f1 a:n>R\nf2 a:n>n;+a 1\nmain>n;0";
    let err = run_err_json(src);
    assert_eq!(
        first_error_line(&err),
        1,
        "error must stay on line 1 (f1), got stderr:\n{err}"
    );
}

#[test]
fn error_on_last_function_with_body_attribution() {
    // Fault on the final function, but with content after it on the same
    // line so the parser hits an in-line boundary, not EOF. (The pure-EOF
    // case `... main a:n>R` falls back to ILO-P008 with a `Span::UNKNOWN`
    // anchor — a pre-existing limitation unrelated to this fix.) Here
    // `main a:n` with no `>type;body` is a same-shape fault as `f2 a:n`
    // earlier in the file: ILO-P020 must anchor at line 3.
    let src = "f1 a:n>n;+a 1\nf2 a:n>n;-a 1\nmain a:n\n";
    let err = run_err_json(src);
    assert_eq!(
        first_error_line(&err),
        3,
        "error must land on line 3 (main), got stderr:\n{err}"
    );
}

#[test]
fn valid_multi_function_file_still_parses() {
    // Sanity: the boundary checks must not reject well-formed multi-fn
    // input. Two helpers and a main, all on their own lines, with normal
    // headers and bodies.
    run_ok("inc a:n>n;+a 1\ndec a:n>n;-a 1\nmain>n;inc 1");
}

#[test]
fn valid_indented_continuation_still_parses() {
    // `normalize_newlines` turns an indented continuation into a `;`, so a
    // function whose body wraps onto the next line is NOT a decl boundary
    // from the parser's perspective. The boundary check must let this
    // through unchanged. Mirrors the shape from `examples/multiline-bodies.ilo`.
    let src = "f a:n>n\n  b=+a 1\n  *b 2\nmain>n;f 3";
    run_ok(src);
}

#[test]
fn no_param_function_missing_return_type_attributes_correctly() {
    // Zero-param function variant: `f2>` with nothing after the `>`. Same
    // class of fault, different code path (no params to short-circuit).
    let src = "f1>n;1\nf2>\nmain>n;0";
    let err = run_err_json(src);
    assert_eq!(
        first_error_line(&err),
        2,
        "expected error on line 2 (f2), got stderr:\n{err}"
    );
}
