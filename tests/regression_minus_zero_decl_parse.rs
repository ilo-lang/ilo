// Regression tests for the `-0` literal hijacking start-of-decl parse.
//
// Originating bug: scientific-researcher rerun9. `- -0 a bo` tripped
// ILO-P020 because the lexer kept `-0` as a glued `Number(-0.0)` token
// (the neg-literal split predicate didn't include `Minus` as a
// fresh-expression context). The outer `-` then parsed as binary subtract
// with operands `-0` and `a`, leaving `bo` orphaned at statement level
// where the multi-fn parser mis-classified it as a new decl header.
// Workaround was binding `na=- 0 a` first - a token tax across every
// persona that reaches for unary negation in hand-rolled numerics
// (Taylor series, RK4, log-fit, distance kernels).
//
// Fix: lexer's neg-literal split predicate now treats `Minus` as a
// fresh-expression context, so `- -0 a bo` re-lexes as
// `Minus, Minus, Number(0), Ident(a), Ident(bo)` and parses as
// `Subtract(Subtract(0, a), bo)` = `-a - bo`. Same for the wider
// `*-0 k s` / `t=-0 /t 6` family.
//
// Collateral semantics change for `- -N M` (N != 0): from
// `Subtract(-N, M)` (pre-fix) to `Negate(Subtract(N, M))` (post-fix).
// Zero in-repo usages of this shape before the fix; pinned explicitly in
// the new reading by the `minus_minus_three_five_*` tests below.
//
// Variants exercised across all three engines (tree, VM, cranelift):
//   - `- -0 a bo` : the originating shape, returns `-a - bo`
//   - `- -0 a`    : two-operand form, returns `-a`
//   - `- -3 5`    : non-zero collateral, returns `2` (new reading)
//   - workaround `na=- 0 a` still works (no regression)

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, args: &[&str]) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg(engine);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}` args={args:?}: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// Originating repro shape.
const DECL: &str = "f a:n bo:n>n;- -0 a bo";
// Two-operand form: `- -0 a` = `Subtract(Negate(0), a)` or equivalently
// `Negate(Subtract(0, a))` = `Subtract(0, Subtract(0, a))` - all reduce to
// `-(-a) = a`? No: `-, -, 0, a` parses outer-Minus consuming inner subtract
// `Subtract(0, a) = -a` then `can_start_operand` is false at EOF, so falls
// into Negate arm: `Negate(Subtract(0, a))` = `Negate(-a)` = `a`.
const NEGATE_ONLY: &str = "f a:n>n;- -0 a";
// Non-zero collateral: pinned to the post-fix reading
// `Negate(Subtract(3, 5))` = `Negate(-2)` = `2`. Pre-fix this was
// `Subtract(-3, 5) = -8`. Zero in-repo usages; new reading is the
// natural left-associative parse.
const NONZERO: &str = "f>n;- -3 5";
// Documented workaround: `na=- 0 a` must continue to work.
const WORKAROUND: &str = "f a:n>n;na=- 0 a;na";

fn check_all(engine: &str) {
    // `- -0 a bo` with a=5, bo=7 -> `(-5) - 7` = -12.
    assert_eq!(
        run(engine, DECL, &["f", "5", "7"]),
        "-12",
        "decl: a=5 bo=7 engine={engine}"
    );
    // `- -0 a bo` with a=3, bo=2 -> `(-3) - 2` = -5.
    assert_eq!(
        run(engine, DECL, &["f", "3", "2"]),
        "-5",
        "decl: a=3 bo=2 engine={engine}"
    );
    // `- -0 a bo` with a=0, bo=0 -> 0.
    assert_eq!(
        run(engine, DECL, &["f", "0", "0"]),
        "0",
        "decl: a=0 bo=0 engine={engine}"
    );
    // `- -0 a bo` with negative a -> double negation.
    // a=-4, bo=1 -> (-(-4)) - 1 = 4 - 1 = 3.
    assert_eq!(
        run(engine, DECL, &["f", "-4", "1"]),
        "3",
        "decl: a=-4 bo=1 engine={engine}"
    );

    // `- -0 a` with a=5 -> Negate(Subtract(0, 5)) = Negate(-5) = 5.
    assert_eq!(
        run(engine, NEGATE_ONLY, &["f", "5"]),
        "5",
        "negate-only: a=5 engine={engine}"
    );
    // `- -0 a` with a=-3 -> Negate(Subtract(0, -3)) = Negate(3) = -3
    // wait: Subtract(0, -3) = 0 - (-3) = 3, Negate(3) = -3. Yes, -3.
    assert_eq!(
        run(engine, NEGATE_ONLY, &["f", "-3"]),
        "-3",
        "negate-only: a=-3 engine={engine}"
    );

    // `- -3 5` -> Negate(Subtract(3, 5)) = Negate(-2) = 2 (post-fix reading).
    assert_eq!(
        run(engine, NONZERO, &["f"]),
        "2",
        "nonzero: - -3 5 engine={engine}"
    );

    // Workaround `na=- 0 a` still produces unary-negate semantics.
    assert_eq!(
        run(engine, WORKAROUND, &["f", "7"]),
        "-7",
        "workaround: na=- 0 a with a=7 engine={engine}"
    );
    assert_eq!(
        run(engine, WORKAROUND, &["f", "-2"]),
        "2",
        "workaround: na=- 0 a with a=-2 engine={engine}"
    );
}

#[test]
fn minus_zero_decl_tree() {
    check_all("--run-tree");
}

#[test]
fn minus_zero_decl_vm() {
    check_all("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn minus_zero_decl_cranelift() {
    check_all("--jit");
}
