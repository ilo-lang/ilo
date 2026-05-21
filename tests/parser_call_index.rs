// Regression tests for ILO-52: `.N` reattaches to the call result when
// the last argument is a bare identifier (variable reference).
//
// Before this fix, `spl s d .1` parsed as `Call(spl, [Ref(s), Index(Ref(d), 1)])`
// because `parse_atom_body`'s bare-ident branch greedily consumed the `.1`
// via `parse_field_chain` while still inside the outer call's arg loop.
//
// Fix: the `in_call_args` flag suppresses bare-ident field-chain consumption
// during arg collection.  The outer `parse_field_chain` call that follows
// every greedy-call loop in `parse_call_or_atom` and `parse_call_arg`
// then attaches the `.N` to the assembled Call result instead.
//
// Verified-broken cases (must all now produce the same value as the
// explicit-parens equivalent):
//   `spl s d .1`       was Call(spl, [s, Index(d,1)])
//   `at xs i .1`       was Call(at,  [xs, Index(i,1)])
//   `num spl s d .1`   was Call(num, [Call(spl,[s,Index(d,1)])])
//
// Previously-working cases that must not regress:
//   `spl "1.2.3" "." .1`   (literal last arg — already worked)
//   `(spl s d).1`           (explicit parens — always worked)
//   `x.1` standalone        (bare ident not in call args — must still work)
//   `at xs 0 .1`            (literal last arg — already worked)

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
        "ilo {engine} failed for `{entry}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

const ENGINES: &[&str] = &[
    "--vm",
    #[cfg(feature = "cranelift")]
    "--jit",
];

// ILO-52 core case: `spl s d .1` where both args are bare idents.
// Must parse as Index(Call(spl,[s,d]),1) not Call(spl,[s,Index(d,1)]).
#[test]
fn spl_bare_idents_dot_index() {
    // The function splits on the delimiter variable and picks element 1.
    let src = "f s:t d:t>t;spl s d .1";
    for engine in ENGINES {
        assert_eq!(
            run_ok(engine, src, "f", &["hello/world", "/"]),
            "world",
            "engine={engine}"
        );
    }
}

// Confirm equivalence with the explicit-parens workaround.
#[test]
fn spl_bare_idents_matches_parens() {
    let bare = "f s:t d:t>t;spl s d .1";
    let parens = "f s:t d:t>t;(spl s d).1";
    for engine in ENGINES {
        let a = run_ok(engine, bare, "f", &["hello/world", "/"]);
        let b = run_ok(engine, parens, "f", &["hello/world", "/"]);
        assert_eq!(a, b, "engine={engine}: bare={a:?} parens={b:?}");
    }
}

// `at xs i .1` — two bare ident args, pick column 1 from a nested list.
#[test]
fn at_bare_idents_dot_index() {
    let src = "f xs:L L n i:n>n;at xs i .1";
    for engine in ENGINES {
        assert_eq!(
            run_ok(engine, src, "f", &["[[10,20,30],[40,50,60]]", "0"]),
            "20",
            "engine={engine}"
        );
    }
}

// `num spl s d .1` — nested case: spl's bare-ident last arg must not
// steal `.1`; `.1` goes on the spl call result, then num converts it.
#[test]
fn nested_num_spl_bare_idents_dot_index() {
    let src = "f s:t d:t>R n t;num spl s d .1";
    for engine in ENGINES {
        assert_eq!(
            run_ok(engine, src, "f", &["price=42", "="]),
            "42",
            "engine={engine}"
        );
    }
}

// Previously-working: literal last arg. Must not regress.
#[test]
fn spl_literal_last_arg_dot_index_unchanged() {
    let src = "main>t;spl \"1.2.3\" \".\" .1";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "main", &[]), "2", "engine={engine}");
    }
}

// Previously-working: explicit parens. Must not regress.
#[test]
fn explicit_parens_dot_index_unchanged() {
    let src = "f s:t d:t>t;(spl s d).1";
    for engine in ENGINES {
        assert_eq!(
            run_ok(engine, src, "f", &["hello/world", "/"]),
            "world",
            "engine={engine}"
        );
    }
}

// Previously-working: bare ident standalone (not in call args). Must
// still produce Index(Ref(x), 1), not break.
#[test]
fn standalone_bare_ident_dot_index_unchanged() {
    let src = "main>t;xs=spl \"a.b\" \".\";xs.1";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "main", &[]), "b", "engine={engine}");
    }
}

// `at xs 0 .1` — literal index arg (already worked pre-fix). Stays
// as Index(Call(at,[xs,0]),1).
#[test]
fn at_literal_arg_dot_index_unchanged() {
    let src = "f xs:L L n>n;at xs 0 .1";
    for engine in ENGINES {
        assert_eq!(
            run_ok(engine, src, "f", &["[[10,20,30],[40,50,60]]"]),
            "20",
            "engine={engine}"
        );
    }
}
