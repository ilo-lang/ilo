// Expected-failure tests for ilo error codes.
//
// Each test runs a short inline ilo snippet that is expected to exit with
// a non-zero status and emit a specific ILO-XXXX error code to stderr.
//
// Error code reference (from SPEC.md / src/verify.rs):
//   ILO-T001  duplicate function / type definition
//   ILO-T002  duplicate function definition
//   ILO-T003  undefined type
//   ILO-T004  undefined variable
//   ILO-T005  undefined function
//   ILO-T006  arity mismatch (wrong number of arguments)
//   ILO-T007  type mismatch on function argument
//   ILO-T008  return type mismatch
//   ILO-T009  binary operator type mismatch
//   ILO-T010  comparison operator type mismatch
//   ILO-T011  list-append (+= ) element type mismatch
//   ILO-T012  negate applied to non-number
//   ILO-T014  foreach on non-list
//   ILO-T018  field access on non-record
//   ILO-T019  no such field on record type
//   ILO-T025  auto-unwrap (!) on non-Result return type

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Run an inline ilo snippet and return (exit_success, combined stderr).
///
/// Passes the first function name as an explicit entry-point arg so we
/// exercise the execution path (where verify errors gate exit). Inline mode
/// with no entry point is an AST-dump inspection path and intentionally
/// skips verify gating; these tests are about the verifier itself, not the
/// dispatch mode, so we route them through the execution path.
fn run(code: &str) -> (bool, String) {
    // Crude but sufficient for these snippets: the first identifier before
    // a parameter list or `>` is the function name. All snippets here start
    // with the function declaration (possibly preceded by a `type` decl).
    let first_fn_name = first_function_name(code).unwrap_or("f");
    let out = ilo()
        .arg(code)
        .arg(first_fn_name)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn ilo: {e}"));
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    (out.status.success(), stderr)
}

/// Extract the first function name from a snippet by skipping any leading
/// `type {...}` declarations and reading the next identifier. Returns None
/// if no obvious function name is found (caller should default to "f").
fn first_function_name(code: &str) -> Option<&str> {
    let mut rest = code.trim_start();
    // Skip leading `type name{...}` decls.
    while let Some(stripped) = rest.strip_prefix("type ") {
        // Find the closing `}` of the type body.
        let close = stripped.find('}')?;
        rest = stripped[close + 1..].trim_start();
    }
    // Read identifier characters (letters, digits, hyphen).
    let end = rest
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
        .unwrap_or(rest.len());
    if end == 0 { None } else { Some(&rest[..end]) }
}

/// Assert that `code` fails and that stderr contains `expected_code`.
fn assert_error(code: &str, expected_code: &str) {
    let (ok, stderr) = run(code);
    assert!(
        !ok,
        "expected failure for snippet {:?} (code {}), but it succeeded",
        code, expected_code
    );
    assert!(
        stderr.contains(expected_code),
        "expected error code {} in stderr for snippet {:?}\nstderr was:\n{}",
        expected_code,
        code,
        stderr
    );
}

// ---- ILO-T001: duplicate type definition ----

#[test]
fn t001_duplicate_type_definition() {
    // Defining two types with the same name should produce ILO-T001.
    assert_error("type point{x:n;y:n} type point{a:n;b:n} f>n;42", "ILO-T001");
}

// ---- ILO-T002: duplicate function definition ----

#[test]
fn t002_duplicate_function_definition() {
    // Two functions with the same name — second one is a duplicate.
    assert_error("f x:n>n;*x 2 f x:n>n;+x 1", "ILO-T002");
}

// ---- ILO-T003: undefined type ----

#[test]
fn t003_undefined_type_in_parameter() {
    // Parameter references an undeclared type.
    assert_error("f x:widget>n;42", "ILO-T003");
}

// ---- ILO-T004: undefined variable ----

#[test]
fn t004_undefined_variable() {
    // Reference to a variable that has never been bound.
    assert_error("f x:n>n;missing-var", "ILO-T004");
}

// ---- ILO-T005: undefined function ----

#[test]
fn t005_undefined_function_call() {
    // Calling a function that does not exist.
    assert_error("f x:n>n;no-such-func x", "ILO-T005");
}

// ---- ILO-T006: arity mismatch — too few arguments ----

#[test]
fn t006_arity_mismatch_too_few_args() {
    // `g` expects 2 args; caller passes only 1.
    assert_error("g a:n b:n>n;+a b f x:n>n;g x", "ILO-T006");
}

#[test]
fn t006_arity_mismatch_too_many_args() {
    // `g` expects 1 arg; caller passes 2.
    assert_error("g x:n>n;*x 2 f x:n>n;g x x", "ILO-T006");
}

// ---- ILO-T007: argument type mismatch ----

#[test]
fn t007_wrong_argument_type_text_instead_of_number() {
    // `g` expects n; caller passes t.
    assert_error("g x:n>n;*x 2 f s:t>n;g s", "ILO-T007");
}

// ---- ILO-T008: return type mismatch ----

#[test]
fn t008_return_type_mismatch_text_body_number_expected() {
    // Function declared to return n but body is t.
    assert_error(r#"f x:n>n;"hello""#, "ILO-T008");
}

#[test]
fn t008_return_type_mismatch_number_body_text_expected() {
    // Function declared to return t but body is n.
    assert_error("f x:n>t;*x 2", "ILO-T008");
}

// ---- ILO-T009: binary operator type mismatch ----

#[test]
fn t009_add_number_and_text() {
    // `+` requires both operands to have the same type (n/t/L).
    assert_error(r#"f x:n y:t>n;+x y"#, "ILO-T009");
}

#[test]
fn t009_multiply_text_operands() {
    // `*` requires n on both sides.
    assert_error(r#"f x:t y:n>n;*x y"#, "ILO-T009");
}

// ---- ILO-T010: comparison type mismatch ----

#[test]
fn t010_compare_number_and_text() {
    // `>` requires both sides to be the same type (n or t).
    assert_error(r#"f x:n y:t>b;>x y"#, "ILO-T010");
}

// ---- ILO-T011: list-append element type mismatch ----

#[test]
fn t011_append_wrong_element_type() {
    // Appending a t to a L n is a type error.
    assert_error(r#"f>n;xs=[1 2 3];xs=+=xs "hello";len xs"#, "ILO-T011");
}

// ---- ILO-T012: negate applied to non-number ----

#[test]
fn t012_negate_text_value() {
    // Unary `-` only applies to n.
    assert_error(r#"f x:t>n;-x"#, "ILO-T012");
}

// ---- ILO-T014: foreach on non-list ----

#[test]
fn t014_foreach_on_number() {
    // `@v x{...}` where x is n, not L, is an error.
    assert_error("f x:n>n;@v x{v};x", "ILO-T014");
}

// ---- ILO-T018: field access on non-record type ----

#[test]
fn t018_field_access_on_number() {
    // Accessing `.field` on a plain number is an error.
    assert_error("f x:n>n;x.field", "ILO-T018");
}

// ---- ILO-T019: no such field on record ----

#[test]
fn t019_no_such_field_on_record() {
    // `point` has no field `z`.
    assert_error("type point{x:n;y:n} f p:point>n;p.z", "ILO-T019");
}

// ---- ILO-T025: auto-unwrap on non-Result ----

#[test]
fn t025_auto_unwrap_on_non_result_function() {
    // `g` returns n (not R), so `g! x` is an error.
    assert_error("g x:n>n;*x 2 f x:n>n;r=g! x;r", "ILO-T025");
}
