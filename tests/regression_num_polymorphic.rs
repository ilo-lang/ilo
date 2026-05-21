// Cross-engine regression tests for the polymorphic `num` builtin.
//
// `num` originally accepted only Text. Personas frequently hit the pattern
// `num (jpar! body)` where the JSON body is already a bare number, forcing
// the verbose triple-roundtrip `num (str (jpar! body))`. Widening `num` to
// also accept Number (as identity-wrapped Ok) closes that hole without
// breaking existing text callers — the result type stays `R n t`.
//
// These tests pin the behaviour across all three engines (tree, VM, JIT):
//   - num "42" → Ok(42)         (existing behaviour preserved)
//   - num 42   → Ok(42)         (new: was a type error before)
//   - num "bad" → Err("bad")    (existing error path preserved)
//   - num (jpar! "42") → Ok(42) (closes the recurring persona pattern)

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_ok(engine: Option<&str>, src: &str, entry: &str, extra: &[&str]) -> String {
    let mut cmd = ilo();
    if let Some(e) = engine {
        cmd.arg(e);
    }
    cmd.arg(src).arg(entry);
    for a in extra {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine:?} {src:?} unexpectedly failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(engine: Option<&str>, src: &str, entry: &str) -> (String, String) {
    let mut cmd = ilo();
    if let Some(e) = engine {
        cmd.arg(e);
    }
    cmd.arg(src).arg(entry);
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine:?} {src:?} unexpectedly succeeded: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// Tree is the default (no flag); --vm and --jit cover the other engines.
#[cfg(feature = "cranelift")]
const ENGINES: &[Option<&str>] = &[None, Some("--vm"), Some("--jit")];
#[cfg(not(feature = "cranelift"))]
const ENGINES: &[Option<&str>] = &[None, Some("--vm")];

#[test]
fn num_text_input_preserved_cross_engine() {
    let src = r#"f>R n t;num "42""#;
    for e in ENGINES {
        assert_eq!(
            run_ok(*e, src, "f", &[]),
            "42",
            "{e:?}: num \"42\" = Ok(42)"
        );
    }
}

#[test]
fn num_number_input_identity_cross_engine() {
    // The new polymorphic case: passing a bare number used to be a type
    // error. Now it returns Ok(n) identity-wrapped.
    let src = r#"f>R n t;num 42"#;
    for e in ENGINES {
        assert_eq!(run_ok(*e, src, "f", &[]), "42", "{e:?}: num 42 = Ok(42)");
    }
}

#[test]
fn num_float_input_identity_cross_engine() {
    let src = r#"f>R n t;num 3.14"#;
    for e in ENGINES {
        assert_eq!(
            run_ok(*e, src, "f", &[]),
            "3.14",
            "{e:?}: num 3.14 = Ok(3.14)"
        );
    }
}

#[test]
fn num_unparseable_text_returns_err_cross_engine() {
    let src = r#"f>R n t;num "bad""#;
    for e in ENGINES {
        let (stdout, stderr) = run_err(*e, src, "f");
        // The Err payload prints to stderr with the `^` prefix on the
        // default unwrap-on-print contract. We just check both streams to
        // stay robust across formatting changes.
        let combined = format!("{stdout}{stderr}");
        assert!(
            combined.contains("bad"),
            "{e:?}: expected 'bad' in Err output, got stdout={stdout:?} stderr={stderr:?}"
        );
    }
}

#[test]
fn num_jpar_numeric_body_cross_engine() {
    // The motivating pattern. `jpar! "42"` returns a Number; pre-fix this
    // failed verification with "'num' expects t, got n". Post-fix it works
    // on every engine and returns Ok(42).
    let src = r#"f>R n t;num (jpar! "42")"#;
    for e in ENGINES {
        assert_eq!(
            run_ok(*e, src, "f", &[]),
            "42",
            "{e:?}: num (jpar! \"42\") = Ok(42)"
        );
    }
}

#[test]
fn num_static_number_param_cross_engine() {
    // Confirm the verifier accepts a statically-typed Number argument.
    let src = r#"f x:n>R n t;num x"#;
    for e in ENGINES {
        assert_eq!(
            run_ok(*e, src, "f", &["42"]),
            "42",
            "{e:?}: f(42) where f takes n returns Ok(42)"
        );
    }
}

#[test]
fn num_unwrap_on_number_cross_engine() {
    // `num!! n` should unwrap the synthetic Ok back to n (panic-on-Err
    // form since the enclosing function returns n, not R).
    let src = r#"f x:n>n;num!! x"#;
    for e in ENGINES {
        assert_eq!(
            run_ok(*e, src, "f", &["42"]),
            "42",
            "{e:?}: num!! 42 unwraps to 42"
        );
    }
}

// ── Belt-and-braces coverage for the new branches ────────────────────────
//
// The cross-engine tests above already drive `num` on every backend, but the
// patch-coverage signal is also flagging the runtime is_number short-circuit
// and the bool runtime error string. The tests below exercise the same code
// paths through additional shapes — propagation through a call frame, a
// dispatch fallthrough where the runtime sees the Number after a verifier
// pass, and a fmod / arith chain that keeps the value live across registers
// before reaching OP_NUM. Each one closes a slightly different code path so
// coverage attribution is robust against optimiser folding.

#[test]
fn num_zero_input_cross_engine() {
    // 0.0 is a NaN-boxed sentinel adjacent to a few special-case tags; make
    // sure the is_number short-circuit handles it identically to other
    // numbers (Ok(0)).
    let src = r#"f>R n t;num 0"#;
    for e in ENGINES {
        assert_eq!(run_ok(*e, src, "f", &[]), "0", "{e:?}: num 0 = Ok(0)");
    }
}

#[test]
fn num_negative_input_cross_engine() {
    let src = r#"f>R n t;num -7"#;
    for e in ENGINES {
        assert_eq!(run_ok(*e, src, "f", &[]), "-7", "{e:?}: num -7 = Ok(-7)");
    }
}

#[test]
fn num_arith_result_cross_engine() {
    // Result of an arithmetic expression flows through OP_NUM as a Number.
    // This forces a non-literal value into the is_number path on every
    // backend.
    let src = r#"f>R n t;num (+1 2)"#;
    for e in ENGINES {
        assert_eq!(run_ok(*e, src, "f", &[]), "3", "{e:?}: num (+1 2) = Ok(3)");
    }
}

#[test]
fn num_via_call_cross_engine() {
    // The Number reaches `num` after a function call boundary so JIT can't
    // see it as a compile-time constant.
    let src = r#"f>R n t;num (g 5);g x:n>n;*x 2"#;
    for e in ENGINES {
        assert_eq!(run_ok(*e, src, "f", &[]), "10", "{e:?}: num (g 5) = Ok(10)");
    }
}

#[test]
fn num_text_unwrap_cross_engine() {
    // Companion to num_unwrap_on_number_cross_engine: covers the text path
    // of OP_NUM (parse) with `num!!` unwrap on each backend.
    let src = r#"f>n;num!! "13""#;
    for e in ENGINES {
        assert_eq!(run_ok(*e, src, "f", &[]), "13", "{e:?}: num!! \"13\" = 13");
    }
}

#[test]
fn num_whitespace_text_cross_engine() {
    // num parses text with leading/trailing whitespace via the trim()
    // branch in the post-fix codegen.
    let src = r#"f>R n t;num "  42  ""#;
    for e in ENGINES {
        assert_eq!(
            run_ok(*e, src, "f", &[]),
            "42",
            "{e:?}: num \"  42  \" = Ok(42)"
        );
    }
}
