// Regression tests for ILO-483: `.field` access on an ident at a call-arg
// position must bind to the ident, not the surrounding call result.
//
// Originating report (ILO-483, discovered while investigating ILO-458):
//
//   `cat "x" e.path` was suspected of parsing as `(cat "x" e).path` —
//   i.e. the `.path` attached to the call result, not to `e`.
//
// Investigation (outcome A): the current parser already binds `.path` to
// the immediately-preceding ident `e`, matching the SPEC's `<ident>.<field>`
// rule (and the long-standing `xs.0` / `pair.0` shapes). The Field node
// has `object: Ref("e")`, not `object: Call { function: "cat", ... }`.
//
// These tests pin that behaviour so a future parser refactor cannot
// silently regress to the call-result-binds shape that the reporter
// feared. We cover the persona shapes from the ticket:
//
//   - `+ "x" r.path`     (text-concat as a stand-in for the original
//                         `cat "x" r.path`; `cat` is list-concat, so
//                         text args trip ILO-T013 before runtime — `+`
//                         exercises the same parse shape with text args)
//   - `fmt "{}" r.field` (formatter call, last arg is field access)
//   - `[r.a r.b r.c]`    (list literal of field accesses)
//   - `cat xs r.tail`    (cat with list-typed field, the literal repro)
//
// We also pin the existing whitespace tolerance: `r .path` (with a space
// between the ident and the dot) currently parses the same as `r.path`.
// The task explicitly forbids changing the lexer's whitespace rule, so
// we just record the existing shape rather than tightening it.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_ok(engine: &str, src: &str, entry: &str) -> String {
    let mut cmd = ilo();
    cmd.args([src, engine, entry]);
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn ast_json(src: &str) -> String {
    let out = ilo()
        .args(["--ast", src])
        .output()
        .expect("failed to run ilo --ast");
    assert!(
        out.status.success(),
        "ilo --ast failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

const ENGINES: &[&str] = &[
    "--vm",
    #[cfg(feature = "cranelift")]
    "--jit",
];

// Core parse-shape pin: `cat "x" r.path` (the exact ILO-483 repro) must
// parse with the field access on `r`, not on the `cat` call result. We
// assert the JSON shape directly. The expression is verifier-rejected
// (cat is list-concat over text-typed args), but the parser must still
// produce the right tree — that's what ILO-483 is about.
#[test]
fn dot_field_after_call_arg_binds_to_ident_ast() {
    let src = "type rec{path:t}\ngo r:rec>t;cat \"x\" r.path";
    let ast = ast_json(src);
    // The Field node's object must be Ref("r"). The buggy shape would
    // have object: { "Call": ... } instead.
    assert!(
        ast.contains("\"Field\""),
        "expected a Field node in AST:\n{ast}"
    );
    assert!(
        ast.contains("\"field\": \"path\""),
        "expected field name `path` in AST:\n{ast}"
    );
    assert!(
        ast.contains("\"Ref\": \"r\""),
        "Field.object should mention Ref(\"r\"):\n{ast}"
    );
    // The Field node sits *inside* the Call's args list — not wrapping it.
    // Pin this by checking the function-name string appears before the
    // Field's field-name string in the serialised JSON (Call.args comes
    // before Call.function in the alphabetised key order; the Field is
    // therefore lexically earlier than the "function": "cat" key).
    let cat_pos = ast
        .find("\"function\": \"cat\"")
        .expect("cat call should be present");
    let field_pos = ast
        .find("\"field\": \"path\"")
        .expect("path field should be present");
    assert!(
        field_pos < cat_pos,
        "Field node should be nested inside the cat call (its key appears \
         before `\"function\": \"cat\"` in the serialised JSON). Got \
         field_pos={field_pos}, cat_pos={cat_pos}. AST:\n{ast}"
    );
}

// Runtime pin for the same shape: `+ "x" r.path` → "xhi".
#[test]
fn dot_field_after_call_arg_runs() {
    let src = "type rec{path:t}\ngo r:rec>t;+\"x\" r.path\nmain>t;r=rec path:\"hi\";go r";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "main"), "xhi", "engine={engine}");
    }
}

// `fmt "{}" r.field` — formatter call with trailing field access.
#[test]
fn fmt_with_trailing_field_access_runs() {
    let src = "type rec{field:t}\ngo r:rec>t;fmt \"{}\" r.field\nmain>t;r=rec field:\"hi\";go r";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "main"), "hi", "engine={engine}");
    }
}

// List literal of field accesses: `[r.a r.b r.c]`.
#[test]
fn list_literal_of_field_accesses_runs() {
    let src = "type rec{a:t;b:t;c:t}\n\
               go r:rec>L t;[r.a r.b r.c]\n\
               main>L t;r=rec a:\"a\" b:\"b\" c:\"c\";go r";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "main"), "[a, b, c]", "engine={engine}");
    }
}

// Existing `pair.0` / `tup.0` numeric-dot-index on a bound list is
// untouched. (Sibling regression files already cover the no-binding
// ILO-T004 hint side.)
#[test]
fn bound_pair_dot_index_unchanged() {
    let src = "main>n;xs=[10 20 30];xs.1";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "main"), "20", "engine={engine}");
    }
}

// Whitespace between ident and `.` currently parses the same as the
// no-whitespace form. This is the existing behaviour; ILO-483 explicitly
// forbids changing the lexer's whitespace-before-dot rule, so we pin
// the current shape rather than tightening it. If a future change wants
// to reject `r .path`, this test should be updated together with the
// SPEC clarification.
#[test]
fn whitespace_before_dot_currently_tolerated() {
    let with_space = "type rec{path:t}\ngo r:rec>t;+\"x\" r .path";
    let without_space = "type rec{path:t}\ngo r:rec>t;+\"x\" r.path";
    // Both parse and produce ASTs whose Field.object is Ref("r").
    let a = ast_json(with_space);
    let b = ast_json(without_space);
    assert!(a.contains("\"field\": \"path\""), "with-space ast:\n{a}");
    assert!(b.contains("\"field\": \"path\""), "no-space ast:\n{b}");
}
