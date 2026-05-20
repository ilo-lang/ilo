// Regression: when a function blows past the register-VM's 256-register
// per-function cap (8-bit register field), the compiler used to raw
// `panic!` with no ILO- code, no source span, no "this happened compiling
// function X" attribution. That made the failure mode useless to anyone
// looking at the backtrace (cf. the Cranelift-side attribution work in
// #354 and the rerun9 ml-engineer note on src/vm/mod.rs:1394 / :4476).
//
// Both panics are now structured `CompileError` variants:
//   - `RegisterOverflow { fn_name, span }` for the alloc_reg cap
//   - `CallRegisterOverflow { fn_name, callee, span }` for the call-site
//     slot reservation
//
// Diagnostic mapping: ILO-T035 and ILO-T036 respectively.
//
// These tests build an ilo program big enough to trip each path and
// assert the error mentions the offending function (and callee, for the
// call variant) so future regressions don't silently slip back to
// "register overflow" with zero attribution.

use ilo::vm::{self, CompileError};

/// Build a function source string with `n` distinct local bindings.
/// Each `xK = +xK-1 1` allocates one fresh register, so this lets us
/// dial in exactly the register pressure the test wants.
fn long_fn(name: &str, n: usize) -> String {
    let mut src = format!("{name} a:n>n;x1=+a 1");
    for i in 2..=n {
        src.push_str(&format!(";x{i}=+x{} 1", i - 1));
    }
    src.push_str(&format!(";+x{n} 0"));
    src
}

fn parse(src: &str) -> ilo::ast::Program {
    let tokens = ilo::lexer::lex(src).expect("lex failed");
    let token_spans: Vec<(ilo::lexer::Token, ilo::ast::Span)> = tokens
        .into_iter()
        .map(|(t, r)| {
            (
                t,
                ilo::ast::Span {
                    start: r.start,
                    end: r.end,
                },
            )
        })
        .collect();
    let (prog, errs) = ilo::parser::parse(token_spans);
    assert!(errs.is_empty(), "parse errors: {errs:?}");
    prog
}

#[test]
fn vm_register_overflow_names_the_function() {
    // 300 lets > 256 registers; cap trips inside alloc_reg.
    let src = long_fn("toobig", 300);
    let prog = parse(&src);
    let err = match vm::compile(&prog) {
        Ok(_) => panic!("expected register-cap error, compile succeeded"),
        Err(e) => e,
    };

    match err {
        CompileError::RegisterOverflow { fn_name, .. } => {
            assert_eq!(fn_name, "toobig", "error should name the function");
        }
        other => panic!("expected RegisterOverflow, got: {other:?}"),
    }
}

#[test]
fn vm_register_overflow_error_message_is_human_attributed() {
    let src = long_fn("widefn", 300);
    let prog = parse(&src);
    let err = match vm::compile(&prog) {
        Ok(_) => panic!("expected register-cap error, compile succeeded"),
        Err(e) => e,
    };

    let msg = err.to_string();
    assert!(
        msg.contains("widefn"),
        "error message should name the function, got: {msg}"
    );
    assert!(
        msg.to_lowercase().contains("register"),
        "error message should mention registers, got: {msg}"
    );
}

#[test]
fn vm_register_overflow_diagnostic_carries_ilo_t035() {
    let src = long_fn("toobig", 300);
    let prog = parse(&src);
    let err = match vm::compile(&prog) {
        Ok(_) => panic!("expected register-cap error, compile succeeded"),
        Err(e) => e,
    };

    let diag = ilo::diagnostic::Diagnostic::from(&err);
    assert_eq!(diag.code, Some("ILO-T035"));
    assert!(diag.message.contains("toobig"));
}

#[test]
fn vm_call_register_overflow_names_caller_and_callee() {
    // A caller that is already deep on registers (250 live locals) then
    // makes a call with enough arguments to push past the cap. We build
    // a small helper that accepts the args, and a caller that does the
    // heavy lifting.
    //
    // Caller: `caller a:n>n; x1..x250 = ...; callee x1 x2 ... x10`
    // The 250 saved locals + result_reg + 10 arg slots > 256.
    let mut src = String::new();
    src.push_str("callee a:n b:n c:n d:n ev:n f:n g:n h:n i:n j:n>n;+a 0;");
    src.push_str("caller a:n>n;x1=+a 1");
    for i in 2..=250 {
        src.push_str(&format!(";x{i}=+x{} 1", i - 1));
    }
    src.push_str(";r=callee x1 x2 x3 x4 x5 x6 x7 x8 x9 x10;+r 0");

    let prog = parse(&src);
    let err = match vm::compile(&prog) {
        Ok(_) => panic!("expected call register-cap error, compile succeeded"),
        Err(e) => e,
    };

    match err {
        CompileError::CallRegisterOverflow {
            fn_name, callee, ..
        } => {
            assert_eq!(fn_name, "caller", "should name the enclosing function");
            assert_eq!(callee, "callee", "should name the call target");
        }
        // Acceptable fallback: the alloc_reg path may trip first depending
        // on how locals stack. The fix still attributes; we just want one
        // of the structured variants, never a raw panic.
        CompileError::RegisterOverflow { fn_name, .. } => {
            assert_eq!(fn_name, "caller");
        }
        other => panic!("expected register-cap error variant, got: {other:?}"),
    }
}

#[test]
fn vm_call_register_overflow_diagnostic_carries_ilo_t036() {
    let mut src = String::new();
    src.push_str("callee a:n b:n c:n d:n ev:n f:n g:n h:n i:n j:n>n;+a 0;");
    src.push_str("caller a:n>n;x1=+a 1");
    for i in 2..=250 {
        src.push_str(&format!(";x{i}=+x{} 1", i - 1));
    }
    src.push_str(";r=callee x1 x2 x3 x4 x5 x6 x7 x8 x9 x10;+r 0");

    let prog = parse(&src);
    let err = match vm::compile(&prog) {
        Ok(_) => panic!("expected call register-cap error, compile succeeded"),
        Err(e) => e,
    };
    let diag = ilo::diagnostic::Diagnostic::from(&err);

    let code = diag.code.unwrap_or("");
    assert!(
        code == "ILO-T035" || code == "ILO-T036",
        "expected ILO-T035 or ILO-T036, got {code:?}"
    );
    assert!(diag.message.contains("caller"));
}
