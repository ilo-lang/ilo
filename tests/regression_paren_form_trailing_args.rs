// Regression: ILO-544 — a paren-form call rejected ANY trailing operand,
// glued or spaced: `fmt2(x)2` and `fmt2(x) 2` both died with ILO-P001 while
// `fmt2(x, 2)` and `fmt2 (x) 2` worked. Models emit the glued shape
// constantly (pipeline-report failed 5/5 on it in the ILO-364 N=5 run,
// ~1000 wasted repair tokens per failure).
//
// Fix: at expression head, a completed adjacent-paren call keeps collecting
// trailing operands with the same greedy loop the spaced postfix form uses,
// so glued and spaced parse identically. Backwards compatible: a trailing
// operand after a paren-form call was previously always a hard error.

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

fn engines() -> Vec<&'static [&'static str]> {
    vec![&[], &["--vm"], &["--jit"]]
}

/// All four spellings of the same 2-arg call must agree, on every engine.
#[test]
fn four_spellings_agree_all_engines() {
    for eng in engines() {
        for src in [
            "main>t;fmt2(+1 2)2",
            "main>t;fmt2(+1 2) 2",
            "main>t;fmt2(+1 2, 2)",
            "main>t;fmt2 (+1 2) 2",
        ] {
            let mut args: Vec<&str> = eng.to_vec();
            args.push(src);
            let (ok, out, err) = run(&args);
            assert!(ok, "engine {eng:?} `{src}`: expected success, stderr={err}");
            assert_eq!(out.trim(), "3.00", "engine {eng:?} `{src}`");
        }
    }
}

/// The bench shape that failed 5/5: format a computed value to 2 dp.
#[test]
fn bench_pipeline_report_shape() {
    let (ok, out, err) = run(&["main>t;s=fmt2(3.14159)2;s"]);
    assert!(ok, "stderr={err}");
    assert_eq!(out.trim(), "3.14");
}

/// Known-arity callee: `at([5 6 7])1` completes to `at [5 6 7] 1`.
#[test]
fn known_arity_completion() {
    let (ok, out, err) = run(&["main>n;at([5 6 7])1"]);
    assert!(ok, "stderr={err}");
    assert_eq!(out.trim(), "6");
}

/// Completion result composes as a prefix-op operand.
#[test]
fn completion_inside_prefix_op() {
    let (ok, out, err) = run(&["main>n;+at([5 6 7])1 10"]);
    assert!(ok, "stderr={err}");
    assert_eq!(out.trim(), "16");
}

/// A COMPLETE paren-form call must not eat following tokens: here the call
/// is an arg to `prnt`, and nothing follows to steal — pin the plain shape.
#[test]
fn complete_call_stays_tight() {
    let (ok, out, err) = run(&["main>_;prnt fmt2(3.14159, 2)"]);
    assert!(ok, "stderr={err}");
    assert_eq!(out.trim(), "3.14");
}

/// Nested paren-form args unchanged.
#[test]
fn nested_paren_calls_unchanged() {
    let src = "main>t;g(f(1), 2)\nf x:n>n;+x 1\ng a:n b:n>t;fmt2(+a b)1";
    let (ok, out, err) = run(&[src]);
    assert!(ok, "stderr={err}");
    assert_eq!(out.trim(), "4.0");
}

/// Field chain after a paren call is a value, not an open arg list —
/// `f(x).0` followed by an operand must NOT extend f's args.
#[test]
fn field_chain_closes_the_arg_list() {
    // pair returns a list; .0 indexes it. The chained value must be the
    // atom — not an arg list reopened for extension.
    let src = "main>n;v=pair(9).0;v\npair x:n>L n;[x 8]";
    let (ok, out, err) = run(&[src]);
    assert!(ok, "stderr={err}");
    assert_eq!(out.trim(), "9");
}
