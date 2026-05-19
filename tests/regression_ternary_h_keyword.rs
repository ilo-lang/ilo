// Cross-engine regression tests for the `?h cond a b` general
// prefix-ternary keyword form.
//
// Before this fix, `?h` was only the bool-subject sugar from PR #330,
// taking exactly two operand atoms (`?h a b` → `if h then a else b`).
// Personas reaching for a 3-arg form analogous to `?=`/`?>` (e.g.
// `sc1=?h cn "metrics:nil" "metrics:ok"`) tripped because the parser
// consumed `h` as the subject ident, then ate two operands and left
// the third stranded: `ILO-P003 expected RBrace, got Text(...)`.
//
// The fix promotes literal subject ident `h` to a fixed prefix-ternary
// keyword when three operand atoms follow: `?h cond then else` desugars
// to `Expr::Ternary { condition: cond, then_expr: then, else_expr: else }`.
// The 2-operand `?h a b` bool-subject form (PR #330) still wins when
// only two operands follow, and any other subject ident is unaffected.
//
// All three backends already handle `Expr::Ternary` from the existing
// prefix-ternary paths, so no engine-level changes were required, but
// cross-engine pinning catches future drift.

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

#[cfg(feature = "cranelift")]
const ENGINES_ALL: &[&str] = &["--vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--vm"];

// ── Tail-expression position ─────────────────────────────────────────

#[test]
fn h_keyword_ternary_true_cross_engine() {
    let src = "f x:n>t;cn=>x 0;?h cn \"pos\" \"nonpos\"";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["f", "5"]),
            "pos",
            "{engine}: cn=true branch"
        );
    }
}

#[test]
fn h_keyword_ternary_false_cross_engine() {
    let src = "f x:n>t;cn=>x 0;?h cn \"pos\" \"nonpos\"";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["f", "0"]),
            "nonpos",
            "{engine}: cn=false branch"
        );
    }
}

// ── Let-binding RHS (the originating use case) ───────────────────────

#[test]
fn h_keyword_ternary_in_let_rhs_cross_engine() {
    // The security-researcher rerun8 probe: assign the conditional to a
    // local in a let-RHS, with text arms and a comparison-derived bool.
    let src = "f mn:t>t;cn=(=mn \"ok\");sc1=?h cn \"metrics:nil\" \"metrics:ok\";sc1";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["f", "ok"]),
            "metrics:nil",
            "{engine}: cn=true RHS"
        );
        assert_eq!(
            run_ok(engine, src, &["f", "no"]),
            "metrics:ok",
            "{engine}: cn=false RHS"
        );
    }
}

// ── Coexistence with PR #330's 2-operand bool-subject form ───────────

#[test]
fn pr330_two_operand_bool_subject_unchanged_cross_engine() {
    // With only two operands the parser must NOT reinterpret as a 3-arg
    // keyword form: `?h a b` keeps PR #330 semantics (`h` is the subject).
    let src = "f h:b>n;?h 7 9";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "true"]), "7", "{engine}: true");
        assert_eq!(run_ok(engine, src, &["f", "false"]), "9", "{engine}: false");
    }
}

#[test]
fn pr330_two_operand_bool_subject_unchanged_in_let_cross_engine() {
    let src = "f h:b>n;v=?h 10 20;v";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "true"]), "10");
        assert_eq!(run_ok(engine, src, &["f", "false"]), "20");
    }
}

// ── Other subject idents are unaffected ──────────────────────────────

#[test]
fn other_ident_subject_still_two_operand_cross_engine() {
    // `?ready a b` (subject `ready`) takes exactly two operands per #330
    // and a third operand is a parse error — only `?h` promotes to the
    // keyword form. This pins the keyword reading to the literal `h`.
    let src = "f ready:b>n;?ready 1 0";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "true"]), "1");
        assert_eq!(run_ok(engine, src, &["f", "false"]), "0");
    }
}

// ── Symmetry with the existing `?=cond a b` family ───────────────────

#[test]
fn h_keyword_matches_eq_prefix_form_cross_engine() {
    // `?h (=x 0) a b` should produce the same answers as `?=x 0 a b`.
    let h = "f x:n>t;?h =x 0 \"zero\" \"nonzero\"";
    let eqp = "g x:n>t;?=x 0 \"zero\" \"nonzero\"";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, h, &["f", "0"]), "zero");
        assert_eq!(run_ok(engine, eqp, &["g", "0"]), "zero");
        assert_eq!(run_ok(engine, h, &["f", "5"]), "nonzero");
        assert_eq!(run_ok(engine, eqp, &["g", "5"]), "nonzero");
    }
}

// ── Predicate-call condition (the common shape an agent reaches for) ──

#[test]
fn h_keyword_predicate_condition_cross_engine() {
    // Predicate `has` over a literal list. `?h (has ...) "a" "b"` should
    // read as `if has ... then "a" else "b"`. We bind the predicate to a
    // local first so the condition is a bare ref atom — keeping every
    // operand a single atom keeps the disambiguator simple.
    let src = "f t:t>t;ok=has [\"a\" \"b\" \"c\"] t;?h ok \"yes\" \"no\"";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "b"]), "yes");
        assert_eq!(run_ok(engine, src, &["f", "z"]), "no");
    }
}

// ── Match shapes must still parse as match ───────────────────────────

#[test]
fn match_arms_on_h_subject_still_work_cross_engine() {
    // `?h{true:a;false:b}` is explicit-arm match (colons + semis), not
    // a ternary. The new keyword path triggers only when no `{` follows
    // the subject.
    let src = "f h:b>t;?h{true:\"yes\";false:\"no\"}";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "true"]), "yes");
        assert_eq!(run_ok(engine, src, &["f", "false"]), "no");
    }
}

#[test]
fn brace_ternary_on_h_subject_still_works_cross_engine() {
    // `?h{a}{b}` brace sugar from #323 stays as bool-subject ternary —
    // brace shape always wins over the prefix-ternary detector.
    let src = "f h:b>n;?h{1}{0}";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "true"]), "1");
        assert_eq!(run_ok(engine, src, &["f", "false"]), "0");
    }
}
