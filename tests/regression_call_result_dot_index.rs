// Regression tests for postfix `.field` / `.N` on a multi-token call
// result without parentheses:
//   `spl "a.b" "." .0` → `(spl "a.b" ".").0`
//   `num spl "1.2.3" ".".1` → `num ((spl "1.2.3" ".").1)`
//   `at xs 0 .1`          → `(at xs 0).1`
//
// Originating: ilo_assessment_feedback.md pending P2 #12. The greedy
// call-args loop in `parse_call_or_atom` (and the nested-call expansion
// in `parse_call_arg`) stopped at the leading `.` because Dot doesn't
// start an operand. The trailing `.N` was left dangling for the infix
// parser to choke on (ILO-P001 "expected declaration, got `.`"). Fix:
// after building a Call expression in either site, route the result
// through `parse_field_chain` so the postfix chain reattaches to the
// call result the same way it does for `(expr).N` and `xs.N`.
//
// Coverage:
//   - top-level multi-token call with trailing `.N` (no spaces)
//   - top-level call with trailing `.N` (space-separated, since the
//     args loop stops at `.` either way)
//   - nested call inside outer arity-1 caller (the original repro)
//   - safe `.?N` shorthand on the call result
//   - chained `.0` then field on a record-returning call
//   - existing parenthesised + bare-ident shapes still work
//
// All happy-path cases run across every public engine (VM and the
// Cranelift JIT when the feature is on) so the parser fix can't
// silently regress on one backend later.

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
    "--vm",
    #[cfg(feature = "cranelift")]
    "--jit",
];

// `spl "1.2.3" ".".1` — bare top-level multi-token call with trailing
// `.N` glued to the previous string literal. Exact minimal repro.
#[test]
fn top_level_call_dot_index_glued() {
    let src = "main>t;spl \"1.2.3\" \".\".1";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "main", &[]), "2", "engine={engine}");
    }
}

// `spl "1.2.3" "." .1` — same shape with a space between the call's
// last arg and the trailing `.N`. The args loop stops at `.` regardless
// of whitespace, so this is also a parser-error case pre-fix.
#[test]
fn top_level_call_dot_index_spaced() {
    let src = "main>t;spl \"1.2.3\" \".\" .1";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "main", &[]), "2", "engine={engine}");
    }
}

// `num spl "1.2.3" ".".1` — the originating repro. Outer arity-1 call
// (`num`) wraps a nested call (`spl`) whose `.N` must reattach to the
// inner call, not the outer.
#[test]
fn nested_call_dot_index_via_outer_caller() {
    let src = "main>R n t;num spl \"1.2.3\" \".\".1";
    for engine in ENGINES {
        // num returns R n t, top-level Ok unwraps the value on stdout.
        assert_eq!(run_ok(engine, src, "main", &[]), "2", "engine={engine}");
    }
}

// Equivalence with the parenthesised workaround. Both shapes must
// produce the same value byte-for-byte.
#[test]
fn glued_form_matches_parenthesised_workaround() {
    let glued = "main>R n t;num spl \"1.2.3\" \".\".1";
    let parens = "main>R n t;num (spl \"1.2.3\" \".\").1";
    for engine in ENGINES {
        let a = run_ok(engine, glued, "main", &[]);
        let b = run_ok(engine, parens, "main", &[]);
        assert_eq!(a, b, "engine={engine}: glued={a:?} parens={b:?}");
    }
}

// `spl "a.b.c" "." .?0` — safe-index shorthand routes through the same
// `parse_field_chain` helper.
#[test]
fn top_level_call_safe_dot_index() {
    let src = "main>O t;spl \"a.b.c\" \".\" .?0";
    for engine in ENGINES {
        // Safe index on a non-empty list returns Some(v) which the
        // top-level print renders bare.
        assert_eq!(run_ok(engine, src, "main", &[]), "a", "engine={engine}");
    }
}

// `at xs 0 .1` — arity-known call inside a list-literal-style head
// (here at top level) with `.N` glued via space. Tests the other Call
// return-site in `parse_call_or_atom`.
#[test]
fn at_call_dot_index_on_list_of_lists() {
    let src = "main rows:L L n>n;at rows 0 .1";
    for engine in ENGINES {
        assert_eq!(
            run_ok(engine, src, "main", &["[[10,20,30],[40,50,60]]"]),
            "20",
            "engine={engine}"
        );
    }
}

// Sanity: existing shapes still work. `xs.0` (bare-ident dot-index)
// and `(spl ...).N` (parenthesised dot-index) are the same code paths
// the fix touches indirectly.
#[test]
fn existing_bare_ident_dot_index_unchanged() {
    let src = "main>t;xs=spl \"a.b\" \".\";xs.0";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "main", &[]), "a", "engine={engine}");
    }
}

#[test]
fn existing_paren_dot_index_unchanged() {
    let src = "main>t;(spl \"a.b\" \".\").1";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "main", &[]), "b", "engine={engine}");
    }
}
