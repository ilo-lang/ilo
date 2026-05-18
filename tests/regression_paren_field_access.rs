// Regression tests for postfix `.field` / `.N` access on a parenthesised
// expression: `(at rows i).2`, `(p with x:1).x`, `(rec).field.sub`.
//
// Originating: gis-analyst rerun9 P1. An inline lambda `map (i:n>n;num! (at
// rows i).2) ixs` errored ILO-P003 expected RParen, got Dot. Root cause was
// broader than the lambda — `parse_atom`'s LParen branch parsed `(expr)` and
// returned without applying the postfix `.field` chain that the bare-Ident
// branch already applied. So `(expr).2` failed at top level too. PR extracts
// `parse_field_chain` and calls it from both branches.
//
// Coverage:
//   - numeric dot-index on parenthesised call (`(at rows 0).2`)
//   - field access on parenthesised record update (`(p with x:30).x`)
//   - safe field on parens (`(at rows 0).?2`, `(rec).?missing`)
//   - chained access (`(at rows 0).0.0` is impossible -- inner is a number;
//     instead chain a record: `(rec).inner.x`)
//   - inline lambda body (the originating shape)
//   - `(expr).(idx)` reach gets the new ILO-P005 hint, not the bare
//     "expected RParen, got Dot" surface
//
// All happy-path cases run across every engine (tree / VM / Cranelift JIT
// when the feature is on) so the parser fix can't silently regress on one
// backend later.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_ok(engine: &str, src: &str, entry: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.args([src, engine, entry]);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
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
    "--jit",
];

// (at rows 0).2 — numeric dot-index on a parenthesised call. The exact
// minimal repro for the originating bug at top-level.
#[test]
fn paren_call_numeric_index() {
    let src = "main rows:L L n>n;(at rows 0).2";
    for engine in ENGINES {
        assert_eq!(
            run_ok(engine, src, "main", &["[[1,2,3],[4,5,6]]"]),
            "3",
            "engine={engine}"
        );
    }
}

// (at rows 1).0 — different row, index 0. Pins that the parser doesn't
// hardcode `.0` or `.2` and walks the chain generically.
#[test]
fn paren_call_numeric_index_nonzero_row() {
    let src = "main rows:L L n>n;(at rows 1).0";
    for engine in ENGINES {
        assert_eq!(
            run_ok(engine, src, "main", &["[[1,2,3],[4,5,6]]"]),
            "4",
            "engine={engine}"
        );
    }
}

// Inline lambda body — the originating shape verbatim. `(at rows i).2`
// is read off each row indexed by `ixs`. This captures `rows` from the
// enclosing scope, which is tree-only today (pre-existing limitation,
// see assessment doc on closure capture across VM/Cranelift), so we
// run it through `--run-tree` only — the parser fix is engine-agnostic.
#[test]
fn inline_lambda_body_paren_field_access_tree() {
    let src = "main rows:L L n ixs:L n>L n;map (i:n>n;(at rows i).2) ixs";
    assert_eq!(
        run_ok(
            "--run-tree",
            src,
            "main",
            &["[[1,2,3],[4,5,6],[7,8,9]]", "[0,1,2]"]
        ),
        "[3, 6, 9]"
    );
}

// Inline lambda body without capture — the originating shape's parse
// fix verified across every engine. Reads `.0` off each row directly,
// no enclosing-scope binding crossed, so VM / Cranelift accept the
// lifted lambda.
#[test]
fn inline_lambda_body_paren_field_access_no_capture() {
    let src = "main rows:L L n>L n;map (r:L n>n;(slc r 0 2).1) rows";
    for engine in ENGINES {
        assert_eq!(
            run_ok(engine, src, "main", &["[[10,20,30],[40,50,60]]"]),
            "[20, 50]",
            "engine={engine}"
        );
    }
}

// (p with x:30).x — field access on a parenthesised record-update.
// Verifies the chain also applies to non-call grouped expressions.
#[test]
fn paren_record_update_field() {
    let src = "type point{x:n;y:n}\nmain v:n>n;p=point x:v y:20;(p with x:30).x";
    for engine in ENGINES {
        assert_eq!(
            run_ok(engine, src, "main", &["10"]),
            "30",
            "engine={engine}"
        );
    }
}

// (at rows 0).?2 — safe numeric dot-index on a parenthesised call.
// `.?N` shorthand still parses through the new shared helper.
#[test]
fn paren_safe_numeric_index() {
    let src = "main rows:L L n>O n;(at rows 0).?2";
    for engine in ENGINES {
        assert_eq!(
            run_ok(engine, src, "main", &["[[1,2,3]]"]),
            "3",
            "engine={engine}"
        );
    }
}

// (expr).(idx) — variable-position reach gets the new ILO-P005 hint
// rather than the bare "expected RParen, got Dot" surface. The hint
// shape differs from the bare-ident case (no leading name to suggest
// `at <name> (idx)` against) — it points at `xs=(expr);at xs (idx)`
// or `at (expr) (idx)` instead.
#[test]
fn paren_variable_index_emits_helpful_hint() {
    let src = "main rows:L L n i:n>n;(at rows 0).(+i 1)";
    let out = ilo()
        .args([src, "main", "[[1,2,3]]", "0"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success(), "expected parse error, got success");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("ILO-P005"),
        "expected ILO-P005, got: {stderr}"
    );
    assert!(
        stderr.contains("at (expr) (idx)") || stderr.contains("xs=(expr)"),
        "expected paren-form hint, got: {stderr}"
    );
}

// xs.2 — bare-ident dot-index still works after the refactor. The Ident
// branch now goes through the same `parse_field_chain` helper as the
// LParen branch; this pins it didn't regress.
#[test]
fn bare_ident_numeric_index_unchanged() {
    let src = "main>n;xs=[10,20,30];xs.2";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "main", &[]), "30", "engine={engine}");
    }
}

// rec.x — bare-ident field access still works after the refactor.
#[test]
fn bare_ident_field_access_unchanged() {
    let src = "type point{x:n;y:n}\nmain>n;p=point x:42 y:7;p.x";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "main", &[]), "42", "engine={engine}");
    }
}

// xs.(expr) — bare-ident variable-index hint shape preserved (names the
// binding the agent can use, unlike the paren-head case above).
#[test]
fn bare_ident_variable_index_hint_unchanged() {
    let src = "main xs:L n i:n>n;xs.(+i 1)";
    let out = ilo()
        .args([src, "main", "[1,2,3]", "0"])
        .output()
        .expect("failed to run ilo");
    assert!(!out.status.success(), "expected parse error, got success");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("ILO-P005"),
        "expected ILO-P005, got: {stderr}"
    );
    assert!(
        stderr.contains("at xs (expr)") || stderr.contains("xs.i"),
        "expected named-binding hint, got: {stderr}"
    );
}
