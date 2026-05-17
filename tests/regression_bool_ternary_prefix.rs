// Cross-engine regression tests for the bare-bool prefix ternary
// `?subj a b`.
//
// Before this fix, `?h a b` parsed `h` as a match subject, then
// expected `{` for arms and errored with `ILO-P003 expected LBrace,
// got Number(1.0)`. The brace form `?h{a}{b}` worked after #323 and
// the comparison-led prefix form `?=h true a b` always worked, but
// the unbraced bare-bool shape was asymmetric and forced the user
// into one of the longer workarounds, costing extra tokens on every
// conditional with a bare-bool subject.
//
// The fix extends `parse_match_stmt` / `parse_match_expr`: after the
// subject is consumed and the brace-ternary shape detector misses,
// if the next token is not `LBrace` but `can_start_operand()`, two
// operands are parsed and the result is `Expr::Ternary { subj, a, b }`.
// All three backends already handle `Expr::Ternary` from the existing
// `?=cond a b` and `?h{a}{b}` paths, so no engine-level changes were
// needed, but cross-engine pinning catches future drift.

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
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];
#[cfg(not(feature = "cranelift"))]
const ENGINES_ALL: &[&str] = &["--run-tree", "--run-vm"];

// ── Bare-bool prefix ternary as tail expression ──────────────────────

#[test]
fn bool_prefix_ternary_true_cross_engine() {
    let src = "f h:b>n;?h 1 0";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["f", "true"]),
            "1",
            "{engine}: ?h 1 0 on true"
        );
    }
}

#[test]
fn bool_prefix_ternary_false_cross_engine() {
    let src = "f h:b>n;?h 1 0";
    for engine in ENGINES_ALL {
        assert_eq!(
            run_ok(engine, src, &["f", "false"]),
            "0",
            "{engine}: ?h 1 0 on false"
        );
    }
}

#[test]
fn bool_prefix_ternary_string_branches_cross_engine() {
    let src = "f h:b>t;?h \"yes\" \"no\"";
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

// ── Bare-bool prefix ternary in expression position (RHS) ────────────

#[test]
fn bool_prefix_ternary_assign_cross_engine() {
    let src = "f h:b>n;v=?h 10 20;v";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "true"]), "10", "{engine}: true");
        assert_eq!(run_ok(engine, src, &["f", "false"]), "20", "{engine}: false");
    }
}

// ── Arms can be prefix-binop expressions ─────────────────────────────

#[test]
fn bool_prefix_ternary_prefix_op_branches_cross_engine() {
    // Operands use `parse_operand`, so prefix-binop forms work directly:
    // `?h +1 2 *3 4` reads as ternary with then=`+1 2` (3), else=`*3 4` (12).
    let src = "f h:b>n;?h +1 2 *3 4";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "true"]), "3", "{engine}: +1 2");
        assert_eq!(run_ok(engine, src, &["f", "false"]), "12", "{engine}: *3 4");
    }
}

// ── Subject can be a comparison result bound to a local ──────────────

#[test]
fn bool_prefix_ternary_comparison_subject_cross_engine() {
    let src = "f x:n>t;c=>x 0;?c \"pos\" \"nonpos\"";
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

// ── Symmetry: same result as the brace form and the `?=cond` form ────

#[test]
fn bool_prefix_ternary_matches_brace_form_cross_engine() {
    // Both shapes should produce identical results, since both desugar
    // to the same `Expr::Ternary` node.
    let bare = "f h:b>n;?h 7 9";
    let brace = "g h:b>n;?h{7}{9}";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, bare, &["f", "true"]), "7");
        assert_eq!(run_ok(engine, brace, &["g", "true"]), "7");
        assert_eq!(run_ok(engine, bare, &["f", "false"]), "9");
        assert_eq!(run_ok(engine, brace, &["g", "false"]), "9");
    }
}

#[test]
fn bool_prefix_ternary_matches_eq_prefix_form_cross_engine() {
    // `?h 1 0` on bool h is semantically `?=h true 1 0`.
    let bare = "f h:b>n;?h 1 0";
    let eqp = "g h:b>n;?=h true 1 0";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, bare, &["f", "true"]), "1");
        assert_eq!(run_ok(engine, eqp, &["g", "true"]), "1");
        assert_eq!(run_ok(engine, bare, &["f", "false"]), "0");
        assert_eq!(run_ok(engine, eqp, &["g", "false"]), "0");
    }
}

// ── Existing match shapes must keep parsing as match ─────────────────

#[test]
fn match_on_int_arms_still_work_cross_engine() {
    // The new prefix-ternary path triggers only when the next token is
    // NOT `LBrace`; the match-arm form is unchanged.
    let src = "f x:n>t;?x{1:\"a\";2:\"b\";_:\"c\"}";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "1"]), "a", "{engine}: arm 1");
        assert_eq!(run_ok(engine, src, &["f", "2"]), "b", "{engine}: arm 2");
        assert_eq!(run_ok(engine, src, &["f", "9"]), "c", "{engine}: wildcard");
    }
}

#[test]
fn eq_prefix_ternary_unchanged_cross_engine() {
    // `?=x 0 a b` is unaffected by the new path because `?` followed by
    // a comparison operator routes via `is_prefix_ternary` directly.
    let src = "f x:n>t;?=x 0 \"zero\" \"nonzero\"";
    for engine in ENGINES_ALL {
        assert_eq!(run_ok(engine, src, &["f", "0"]), "zero", "{engine}: zero");
        assert_eq!(
            run_ok(engine, src, &["f", "5"]),
            "nonzero",
            "{engine}: nonzero"
        );
    }
}
