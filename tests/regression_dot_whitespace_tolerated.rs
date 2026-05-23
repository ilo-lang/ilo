// Regression test for ILO-486: whitespace between an atom and `.field`
// must parse as field access, producing an AST identical to the
// no-whitespace form.
//
// Design decision (see SPEC.md dot-access section): the dot is NOT
// load-bearing the way `-` is — `.` is not a valid identifier character,
// so there's no parse-time ambiguity between `r .path` and `r . path`.
// Tightening the parser would break existing programs with zero clarity
// gain. The SPEC documents the relaxed rule explicitly; this test pins
// the behaviour so it cannot silently regress.
//
// Sibling note: ILO-483's `regression_dot_field_in_call_arg.rs` also
// covers a whitespace-tolerated case as part of its broader call-arg
// precedence pinning. This test stands alone so the ILO-486 SPEC
// guarantee has a dedicated pin, even if the ILO-483 test moves or
// narrows later.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
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

// Both `r.path` and `r .path` must produce the same AST: a Field node
// with object Ref("r") and field "path". We compare the AST JSON
// directly. Any future parser refactor that either tightens the rule
// (rejects the whitespace form) or changes the AST shape will trip
// this test.
#[test]
fn whitespace_before_dot_field_parses_same_as_tight() {
    let tight = "type rec{path:t}\ngo r:rec>t;r.path";
    let loose = "type rec{path:t}\ngo r:rec>t;r .path";
    let a = ast_json(tight);
    let b = ast_json(loose);
    assert_eq!(
        a, b,
        "AST for `r.path` and `r .path` should be identical.\n\
         tight:\n{a}\nloose:\n{b}"
    );
    // Sanity: both contain the expected Field shape.
    assert!(a.contains("\"Field\""), "expected Field node:\n{a}");
    assert!(
        a.contains("\"field\": \"path\""),
        "expected field name `path`:\n{a}"
    );
    assert!(
        a.contains("\"Ref\": \"r\""),
        "Field.object should be Ref(\"r\"):\n{a}"
    );
}

// Numeric dot-index `xs.0` vs `xs .0` — same parse, list-index shape.
#[test]
fn whitespace_before_dot_index_parses_same_as_tight() {
    let tight = "main>n;xs=[10 20 30];xs.1";
    let loose = "main>n;xs=[10 20 30];xs .1";
    let a = ast_json(tight);
    let b = ast_json(loose);
    assert_eq!(
        a, b,
        "AST for `xs.1` and `xs .1` should be identical.\n\
         tight:\n{a}\nloose:\n{b}"
    );
}

// Safe field accessor `.?field` — same whitespace tolerance.
#[test]
fn whitespace_before_safe_dot_field_parses_same_as_tight() {
    let tight = "type rec{path:t}\ngo r:rec>O t;r.?path";
    let loose = "type rec{path:t}\ngo r:rec>O t;r .?path";
    let a = ast_json(tight);
    let b = ast_json(loose);
    assert_eq!(
        a, b,
        "AST for `r.?path` and `r .?path` should be identical.\n\
         tight:\n{a}\nloose:\n{b}"
    );
}
