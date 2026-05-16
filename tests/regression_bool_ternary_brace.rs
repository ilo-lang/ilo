// Cross-engine regression tests for the bare-bool ternary sugar
// `?subj{a}{b}`.
//
// Before this fix, `?h{a}{b}` was unconditionally routed to match-arm
// parsing. The literal `a` was read as a pattern, the next token `}`
// triggered `ILO-P003 expected Colon, got RBrace`, and the trailing
// `{b}` then errored at the top-level statement parser with
// `ILO-P001 expected declaration, got LBrace`. The only working form
// was the prefix-ternary workaround `?=h true a b`, costing ~5 chars
// on every conditional and flagged across five releases (v0.11.0
// through v0.11.4) as the longest-running streaming-tail papercut.
//
// The fix desugars `?subj{a}{b}` to `Expr::Ternary { subj, a, b }` at
// parse time when the first brace contains a single colon-and-semi-
// free expression and is followed immediately by another brace.
// Match-arm shapes (`?x{1:a;2:b}`, `?x{pat:body}`, etc.) are
// unaffected because the shape detector bails on any `:` or `;` at
// the outer brace depth.
//
// These tests exercise the happy path and the negative cases across
// every engine so the desugaring stays consistent on tree, VM and
// Cranelift — all three backends already handle `Expr::Ternary` from
// the existing prefix-ternary and `=cond{a}{b}` paths, so no engine-
// level changes were needed, but cross-engine pinning catches any
// future drift.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_ok(engine: &str, src: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} {src:?} {args:?} unexpectedly failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(engine: &str, src: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} {src:?} {args:?} unexpectedly succeeded: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm"];

// ── Bare-bool ternary as statement (tail expr) ───────────────────────

#[test]
fn bool_ternary_stmt_true_cross_engine() {
    let src = "f h:b>n;?h{1}{0}";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["f", "true"]),
            "1",
            "{engine}: ?h{{1}}{{0}} on true"
        );
    }
}

#[test]
fn bool_ternary_stmt_false_cross_engine() {
    let src = "f h:b>n;?h{1}{0}";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["f", "false"]),
            "0",
            "{engine}: ?h{{1}}{{0}} on false"
        );
    }
}

#[test]
fn bool_ternary_string_branches_cross_engine() {
    let src = "f h:b>t;?h{\"yes\"}{\"no\"}";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["f", "true"]),
            "yes",
            "{engine}: string then-branch"
        );
        assert_eq!(
            run_ok(engine, src, &["f", "false"]),
            "no",
            "{engine}: string else-branch"
        );
    }
}

// ── Bare-bool ternary in expression position (RHS / call arg) ────────

#[test]
fn bool_ternary_assign_cross_engine() {
    let src = "f h:b>n;v=?h{10}{20};v";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "true"]), "10", "{engine}: true");
        assert_eq!(
            run_ok(engine, src, &["f", "false"]),
            "20",
            "{engine}: false"
        );
    }
}

// ── Arms can contain calls and prefix-op expressions ─────────────────

#[test]
fn bool_ternary_call_branches_cross_engine() {
    let src = "f h:b>n;?h{+1 2}{*3 4}";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "true"]), "3", "{engine}: +1 2");
        assert_eq!(run_ok(engine, src, &["f", "false"]), "12", "{engine}: *3 4");
    }
}

#[test]
fn bool_ternary_nested_call_branches_cross_engine() {
    // The first brace's content is a multi-arg call; the shape detector
    // tolerates whitespace-call sequences because none of them introduce
    // `:` or `;` at the outer brace depth.
    let src = "f h:b>n;?h{sum [1, 2, 3]}{len [4, 5]}";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "true"]), "6", "{engine}: sum");
        assert_eq!(run_ok(engine, src, &["f", "false"]), "2", "{engine}: len");
    }
}

// ── Subject can be a comparison/logical bool expression ──────────────

#[test]
fn bool_ternary_comparison_subject_cross_engine() {
    // `?=x 0` is the existing prefix-ternary path; the new sugar uses a
    // bare expression as condition. Here the subject is a bound bool
    // (function param), exercising the typical use case.
    let src = "f x:n>t;c=>x 0;?c{\"pos\"}{\"nonpos\"}";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["f", "5"]),
            "pos",
            "{engine}: pos branch"
        );
        assert_eq!(
            run_ok(engine, src, &["f", "0"]),
            "nonpos",
            "{engine}: nonpos branch"
        );
    }
}

// ── Negative shape: match-arm form still parses as match ─────────────

#[test]
fn match_on_int_with_colons_still_parses_cross_engine() {
    let src = "f x:n>t;?x{1:\"a\";2:\"b\";_:\"c\"}";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "1"]), "a", "{engine}: arm 1");
        assert_eq!(run_ok(engine, src, &["f", "2"]), "b", "{engine}: arm 2");
        assert_eq!(run_ok(engine, src, &["f", "9"]), "c", "{engine}: wildcard");
    }
}

#[test]
fn match_on_bool_with_colons_still_parses_cross_engine() {
    // `?h{true:a;false:b}` is the canonical exhaustive-bool match form.
    // The ternary shape detector must NOT swallow this — the first brace
    // contains both `:` and `;`, so it falls through to match-arm
    // parsing as before.
    let src = "f h:b>n;?h{true:10;false:20}";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["f", "true"]),
            "10",
            "{engine}: true arm"
        );
        assert_eq!(
            run_ok(engine, src, &["f", "false"]),
            "20",
            "{engine}: false arm"
        );
    }
}

#[test]
fn single_arm_match_with_colon_still_parses() {
    // `?x{1:"one"}` has only one arm with a colon — not a ternary
    // shape (first brace contains `:`), so it goes to match-arm parsing
    // and errors on non-exhaustiveness as before. We assert the error
    // class, not the exact message, to stay robust.
    let src = "f x:n>t;?x{1:\"one\"}";
    for engine in ENGINES_ALL {
        let err = run_err(engine, src, &["f", "1"]);
        assert!(
            err.contains("ILO-T024") || err.contains("non-exhaustive"),
            "{engine}: expected non-exhaustive match error, got: {err}"
        );
    }
}

// ── Negative shape: empty first brace falls back to match-arm ────────

#[test]
fn empty_first_brace_falls_through_to_match() {
    // `?h{}{...}` has an empty first brace — not a valid ternary shape
    // (no then-expression). The shape detector bails and the existing
    // match-arm error surfaces.
    let src = "f h:b>n;?h{}{0}";
    let err = run_err("--run-tree", src, &["f", "true"]);
    // Bare error class, not exact message — robust to wording tweaks.
    assert!(
        err.contains("ILO-P") || err.contains("expected"),
        "expected parser error, got: {err}"
    );
}
