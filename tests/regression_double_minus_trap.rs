//! Regression: the double-minus prefix-binop trap.
//!
//! The shape `- -<op> a b <op> c d`, with each `<op>` a prefix binop in
//! `{+, *, /}` and each followed by two atoms, parses as
//! `-((a OP1 b) - (c OP2 d))` — that is, the inner `-` consumes both
//! prefix-binop groups as its operands, then the outer `-` has nothing
//! left and becomes a unary negate. Algebraically that equals
//! `-(a OP1 b) + (c OP2 d)`: the sign of the second product is flipped
//! relative to the natural reading `-(a OP1 b) - (c OP2 d)`.
//!
//! The verifier sees a valid expression and the evaluator runs it, so the
//! bug is silent — only domain knowledge surfaces it (e.g. a
//! damped-pendulum natural-form transcription `-g*s - b*om` rendered as
//! `- -*gl s *b om`, which evaluates with `+b*om` rather than `-b*om`).
//!
//! Fix: parser rejects this exact shape at parse time with ILO-P021 and a
//! bind-first suggestion. The check is intentionally narrow — single-atom
//! variants like `- -a b` (legitimate negate-of-subtract) and the
//! documented `+*a b c` / `-+a b c` families remain accepted.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_err_json(src: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg("--json").arg(src);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected failure for {src:?}, stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn run_ok(src: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "expected success for {src:?}, stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn first_error_code(stderr: &str) -> String {
    let key = "\"code\":\"";
    let idx = stderr
        .find(key)
        .unwrap_or_else(|| panic!("no code field in stderr:\n{stderr}"));
    let tail = &stderr[idx + key.len()..];
    let end = tail.find('"').expect("unterminated code field");
    tail[..end].to_string()
}

// ─── Trap shapes — must reject with ILO-P021 ────────────────────────────────

#[test]
fn rejects_double_minus_star_star() {
    // The originating shape from the rerun6 damped-pendulum repro:
    //   `-g*s - b*om` written as `- -*gl s *b om`.
    let src = "f gl:n s:n b:n om:n>n;- -*gl s *b om";
    let err = run_err_json(src, &["1", "1", "0.5", "1"]);
    assert_eq!(first_error_code(&err), "ILO-P021");
    assert!(
        err.contains("sign-flipping"),
        "expected sign-flipping wording in:\n{err}"
    );
    assert!(
        err.contains("- 0 +*gl s *b om"),
        "expected concrete bind-first suggestion in:\n{err}"
    );
}

#[test]
fn rejects_double_minus_slash_slash() {
    let src = "f a:n b:n c:n d:n>n;- -/a b /c d";
    let err = run_err_json(src, &["4", "2", "6", "3"]);
    assert_eq!(first_error_code(&err), "ILO-P021");
}

#[test]
fn rejects_double_minus_plus_plus() {
    let src = "f a:n b:n c:n d:n>n;- -+a b +c d";
    let err = run_err_json(src, &["1", "2", "3", "4"]);
    assert_eq!(first_error_code(&err), "ILO-P021");
}

#[test]
fn rejects_double_minus_star_slash_mixed() {
    let src = "f a:n b:n c:n d:n>n;- -*a b /c d";
    let err = run_err_json(src, &["1", "2", "6", "3"]);
    assert_eq!(first_error_code(&err), "ILO-P021");
}

#[test]
fn rejects_double_minus_plus_star_mixed() {
    let src = "f a:n b:n c:n d:n>n;- -+a b *c d";
    let err = run_err_json(src, &["1", "2", "3", "4"]);
    assert_eq!(first_error_code(&err), "ILO-P021");
}

#[test]
fn rejects_double_minus_with_number_atoms() {
    // Atom-start tokens include numbers, not just idents.
    let src = "main>n;- -*2 3 *4 5";
    let err = run_err_json(src, &[]);
    assert_eq!(first_error_code(&err), "ILO-P021");
}

// ─── Non-trap shapes — must still parse cleanly ─────────────────────────────

#[test]
fn accepts_negate_of_subtract_single_atoms() {
    // `- -a b` is negate-of-subtract over two atoms — unambiguous, leave it.
    // `-(5 - 3) = -2`.
    let out = run_ok("f a:n b:n>n;- -a b", &["5", "3"]);
    assert_eq!(out.trim(), "-2");
}

#[test]
fn accepts_minus_minus_three_atoms() {
    // `- -a b c` has no inner prefix-binop. The inner `-` is negate-of-`a`
    // and the outer `-` then subtracts the rest. Don't trip.
    let out = run_ok("f a:n b:n c:n>n;- -a b c", &["5", "3", "2"]);
    // -(-5) - 3 ... actually evaluates per current parser: inner `-a` is
    // unary, outer `- (-a) b` is binary subtract — but `c` is still in scope
    // and gets consumed... just assert it runs successfully without
    // ILO-P021. The exact value is whatever the parser produces today.
    assert!(!out.is_empty());
}

#[test]
fn accepts_single_minus_plus_family() {
    // `-+a b c` is the documented "inner prefix-op binds first" family.
    let out = run_ok("f a:n b:n c:n>n;-+a b c", &["1", "2", "3"]);
    assert_eq!(out.trim(), "0");
}

#[test]
fn accepts_plus_star_family() {
    // `+*a b c` — single leading prefix-op, not the trap.
    let out = run_ok("f a:n b:n c:n>n;+*a b c", &["1", "2", "3"]);
    assert_eq!(out.trim(), "5");
}

#[test]
fn accepts_unary_negation() {
    let out = run_ok("f a:n>n;-a", &["5"]);
    assert_eq!(out.trim(), "-5");
}

#[test]
fn accepts_bind_first_workaround() {
    // The suggested fix in the error hint must itself parse and produce
    // the value the agent originally wanted.
    let out = run_ok(
        "f gl:n s:n b:n om:n>n;- 0 +*gl s *b om",
        &["1", "1", "0.5", "1"],
    );
    assert_eq!(out.trim(), "-1.5");
}

// ─── Near-miss shapes — exercise each early-return in the detector ─────────
//
// Each of these probes one specific bail-out in `check_double_minus_trap` so
// the detector's negative branches all have direct coverage. We don't pin
// outputs (the parser's behaviour on the surrounding shapes isn't what's
// under test); we only assert the trap diagnostic does NOT fire.

fn assert_no_p021(src: &str, args: &[&str]) {
    let mut cmd = ilo();
    cmd.arg("--json").arg(src);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("ILO-P021"),
        "ILO-P021 should NOT fire for {src:?}, stderr:\n{stderr}"
    );
}

#[test]
fn near_miss_no_second_minus() {
    // pos+1 is not `-` → first bail-out
    assert_no_p021("f a:n b:n>n;-+a b", &["1", "2"]);
}

#[test]
fn near_miss_inner_op_is_comparison() {
    // pos+2 is not in {+, *, /} (it's `>`) → second bail-out
    assert_no_p021("f a:n b:n>b;- ->a b", &["1", "2"]);
}

#[test]
fn near_miss_third_token_not_atom() {
    // pos+3 is another prefix op, not an atom → third bail-out
    assert_no_p021("f a:n b:n c:n>n;- -*+a b c", &["1", "2", "3"]);
}

#[test]
fn near_miss_fourth_token_not_atom() {
    // pos+4 is another prefix op, not an atom → fourth bail-out
    assert_no_p021("f a:n b:n c:n>n;- -*a +b c", &["1", "2", "3"]);
}

#[test]
fn near_miss_second_op_missing() {
    // pos+5 is not a prefix-arith op (just runs out of tokens) → fifth bail-out
    assert_no_p021("f a:n b:n>n;- -*a b", &["3", "4"]);
}

#[test]
fn near_miss_second_op_followed_by_op_not_atom() {
    // pos+6 is not an atom → sixth bail-out
    assert_no_p021("f a:n b:n c:n d:n>n;- -*a b *+c d", &["1", "2", "3", "4"]);
}

#[test]
fn near_miss_second_op_only_one_atom() {
    // pos+7 is not an atom (we run out, or hit another op) → seventh bail-out
    assert_no_p021("f a:n b:n c:n>n;- -*a b *c", &["1", "2", "3"]);
}

// ─── Cross-engine coverage ──────────────────────────────────────────────────
//
// The fix is in the parser, which all three engines share. To be defensive
// against a future engine-specific re-parse path, exercise each engine
// explicitly and confirm the trap is rejected before any engine runs.

#[test]
fn rejects_trap_on_all_engines() {
    let src = "f gl:n s:n b:n om:n>n;- -*gl s *b om";
    for backend in ["--run-tree", "--run-vm", "--run-cranelift"] {
        let out = ilo()
            .arg("--json")
            .arg(backend)
            .arg(src)
            .arg("1")
            .arg("1")
            .arg("0.5")
            .arg("1")
            .output()
            .expect("failed to run ilo");
        assert!(
            !out.status.success(),
            "expected failure on {backend}, stdout: {}",
            String::from_utf8_lossy(&out.stdout)
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("ILO-P021"),
            "expected ILO-P021 on {backend}, stderr:\n{stderr}"
        );
    }
}
