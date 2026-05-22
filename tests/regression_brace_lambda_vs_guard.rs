// Regression: `looks_like_brace_lambda` must NOT mistake a braced
// guard body whose first statement starts with a prefix-op `>` for a
// brace-lambda. Before this fix, `@x a{>x 0{m=x}}` was misparsed as
// `@x` collection `a{>x 0{...}}`, where `a{...}` looked like a call to
// `a` with a brace-lambda arg because the parser only scanned for a `>`
// at depth 1 and ignored whether any param ident preceded it.
//
// Test names live across interpreter and VM (the failing tests after
// PR #706/#709/#713 landed); this test asserts the canonical
// find-max-with-braced-guard idiom parses and runs.

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
    let out = cmd.output().expect("run");
    assert!(
        out.status.success(),
        "{engine}: stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[test]
fn braced_guard_inside_foreach_parses_across_engines() {
    let src = "mx xs:L n>n;m=xs.0;@x xs{>x m{m=x}};+m 0";
    for engine in ["--vm", "--jit"] {
        let got = run(engine, src, &["mx", "1,2,5,3"]);
        assert_eq!(got, "5", "{engine}: find-max failed");
    }
}

#[test]
fn brace_lambda_still_works_with_param_idents() {
    // Make sure the brace-lambda shape `{x > body}` is unaffected.
    let src = "main>n;ys=map {x>*x 2} [1 2 3];ys.0";
    let got = run("--vm", src, &[]);
    assert_eq!(got, "2");
}
