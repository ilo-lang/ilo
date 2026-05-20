// Regression tests for ILO-T039: 0-arg function used as a bare reference
// in value position instead of being called.
//
// Before this change, `ts = my-fn` where `my-fn` is a 0-arg user function
// produced a silent type error or a confusing generic diagnostic with no
// actionable suggestion. Agents burned retries figuring out why their
// "timestamp" variable held a function reference instead of a number.
//
// After: ILO-T039 emits a targeted diagnostic pointing at `my-fn()` as
// the correct call form.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

/// Run `ilo check` on inline source and return stderr.
fn check_src(src: &str) -> String {
    use std::io::Write;
    let mut f = tempfile::NamedTempFile::new().expect("tempfile");
    f.write_all(src.as_bytes()).expect("write");
    let path = f.path().to_str().unwrap();
    let out = ilo()
        .args(["check", path])
        .output()
        .expect("failed to run ilo check");
    // check may exit non-zero on errors — we check stderr content
    String::from_utf8_lossy(&out.stderr).into_owned() + &String::from_utf8_lossy(&out.stdout)
}

#[test]
fn zero_arg_fn_used_as_binop_operand() {
    // `get-ts` is 0-arg but used as left operand of `+` — should trigger ILO-T039
    let src = "get-ts>n;now-ms()\nmain>n;+get-ts 100";
    let diag = check_src(src);
    assert!(
        diag.contains("ILO-T039"),
        "expected ILO-T039 for 0-arg fn in binop operand, got: {diag}"
    );
    assert!(
        diag.contains("get-ts()"),
        "hint should mention 'get-ts()' call form, got: {diag}"
    );
}

#[test]
fn zero_arg_fn_in_binop_before_atom_no_expand() {
    // `get-ts` is 0-arg. In prefix binop `+get-ts 100`, `get-ts` appears
    // before `100` so can_start_operand is true and auto-expand does NOT fire.
    // The verifier should emit ILO-T039.
    let src = "get-ts>n;now-ms()\nmain>n;+get-ts 100";
    let diag = check_src(src);
    assert!(
        diag.contains("ILO-T039"),
        "expected ILO-T039 for 0-arg fn before atom in binop, got: {diag}"
    );
}

#[test]
fn zero_arg_fn_called_correctly_no_diagnostic() {
    // `get-ts()` is the correct form — no ILO-T039
    let src = "get-ts>n;now-ms()\nmain>n;get-ts()";
    let diag = check_src(src);
    assert!(
        !diag.contains("ILO-T039"),
        "ILO-T039 should not fire when 0-arg fn is called with (), got: {diag}"
    );
}

#[test]
fn non_zero_arg_fn_ref_no_diagnostic() {
    // HOF refs for >0 param fns — ILO-T039 must NOT fire
    let src = "double f:n>n;*f 2\nmain>L n;map double [1, 2, 3]";
    let diag = check_src(src);
    assert!(
        !diag.contains("ILO-T039"),
        "ILO-T039 must not fire for non-zero-arg fn ref (HOF), got: {diag}"
    );
}
