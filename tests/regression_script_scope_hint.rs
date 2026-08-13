// ILO-546: grade-calculator failed 3/3 in the N=5 benchmark on the
// one-line glued script shape: `sts=[..];ws=[..];grd a:n>t;..;@s sts{..}`
// makes everything after `grd a:n>t;` part of grd's body (same-line = body,
// the documented script-mode rule), so `sts` is genuinely out of scope and
// ILO-T004 fires — but the old hint suggested an enclosing-fn lambda, which
// sends the model the wrong way. The hint now names the one-edit fix.
//
// The multi-line interleaved form was never broken (the ticket's original
// diagnosis) — pinned here so it stays that way.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(args: &[&str]) -> (bool, String, String) {
    let out = ilo()
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn ilo: {e}"));
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

/// Multi-line: bindings, decl mid-stream, loop over earlier bindings —
/// one shared implicit main scope, decl registers as a sibling.
#[test]
fn multiline_interleaved_decl_shares_scope() {
    let src =
        "sts=[85, 92, 78]\nws=[0.5, 0.3, 0.2]\ngrd a:n>t;>=a 90 \"A\";\"B\"\n@s sts{prnt grd s}";
    let (ok, out, err) = run(&[src]);
    assert!(ok, "stderr={err}");
    assert_eq!(out.trim(), "B\nA\nB");
}

/// Fn declared later in the file is callable from earlier script statements
/// (decls are file-scope siblings, order-independent).
#[test]
fn decl_after_statements_is_callable() {
    let src = "xs=[1, 2, 3]\n@x xs{r=dbl x;prnt r}\ndbl n:n>n;*n 2";
    let (ok, out, err) = run(&[src]);
    assert!(ok, "stderr={err}");
    assert_eq!(out.trim(), "2\n4\n6");
}

/// Multiple decls interleaved between statement runs — still one main scope.
#[test]
fn multiple_interleaved_decls() {
    let src = "a=10\ninc n:n>n;+n 1\nb=inc a\ndbl n:n>n;*n 2\nprnt dbl b";
    let (ok, out, err) = run(&[src]);
    assert!(ok, "stderr={err}");
    assert_eq!(out.trim(), "22");
}

/// One-line glued form: T004 fires (correct — same-line joins the body) and
/// the hint names the top-level binding + the own-line fix, NOT the
/// enclosing-fn lambda advisory.
#[test]
fn glued_form_hint_names_the_unglue_fix() {
    let src = "sts=[85, 92, 78];ws=[0.5, 0.3, 0.2];grd a:n>t;>=a 90 \"A\";\"B\";@s sts{prnt grd s}";
    let (ok, _out, err) = run(&[src]);
    assert!(!ok, "glued form must still error");
    assert!(err.contains("ILO-T004"), "stderr={err}");
    assert!(
        err.contains("bound at top level"),
        "hint should name the top-level binding: {err}"
    );
    assert!(
        err.contains("its own line"),
        "hint should name the own-line fix: {err}"
    );
    assert!(
        !err.contains("enclosing fn"),
        "lambda advisory is the wrong steer for this shape: {err}"
    );
}

/// The ILO-504 lambda advisory still fires when the undefined name is NOT a
/// top-level binding (genuine capture attempt).
#[test]
fn lambda_advisory_survives_for_non_main_names() {
    let (ok, _out, err) = run(&["f val:n>n;+val vall"]);
    assert!(!ok);
    assert!(err.contains("ILO-T004"), "stderr={err}");
    assert!(
        err.contains("enclosing fn"),
        "ILO-504 advisory should remain for non-main names: {err}"
    );
}
