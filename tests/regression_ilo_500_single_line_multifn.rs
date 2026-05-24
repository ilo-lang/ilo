// Regression: ILO-500 - single-line `;`-separated multi-fn programs where a
// non-last function's body contains a binding (Let statement) were incorrectly
// rejected with ILO-P024 ("fn declarations are top-level only"). The second
// top-level fn was misread as a nested fn inside the first.
//
// Root cause: parse_body_with checked `has_binding` to decide whether the next
// fn-decl-start was a sibling or nested fn. With a binding present and no
// decl_boundary newline marker (single-line form has neither), it fell through
// to the ILO-P024 error path.
//
// Fix: a fn-decl-start after a `;` at the top level always terminates the
// current body and is treated as a sibling. Nested-capture detection is
// deferred to a verify-time pass.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str) -> (bool, String, String) {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
        String::from_utf8_lossy(&out.stderr).trim().to_string(),
    )
}

// Non-last fn has a binding (`c`), followed by a sibling `main` on the same line.
// `f 5` should return 7 (5 + 2). Uses `c` as the final expression (implicit return).
const BINDING_IN_NON_LAST_FN: &str = "f x:n>n;c=+x 2;c;main>n;f 5";

fn check_binding_in_non_last_fn(engine: &str) {
    let (ok, stdout, stderr) = run(engine, BINDING_IN_NON_LAST_FN, "main");
    assert!(
        ok,
        "engine={engine}: expected success, stderr={stderr}"
    );
    assert_eq!(stdout, "7", "engine={engine}");
}

#[test]
fn binding_in_non_last_fn_vm() {
    check_binding_in_non_last_fn("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn binding_in_non_last_fn_cranelift() {
    check_binding_in_non_last_fn("--jit");
}

// Three sibling fns in single-line form, middle one has a binding.
const THREE_FNS_MIDDLE_BINDING: &str = "add x:n>n;s=+x 1;s;dbl x:n>n;*x 2;main>n;add (dbl 3)";

fn check_three_fns_middle_binding(engine: &str) {
    let (ok, stdout, stderr) = run(engine, THREE_FNS_MIDDLE_BINDING, "main");
    assert!(
        ok,
        "engine={engine}: expected success, stderr={stderr}"
    );
    // dbl 3 = 6, add 6 = 7
    assert_eq!(stdout, "7", "engine={engine}");
}

#[test]
fn three_fns_middle_binding_vm() {
    check_three_fns_middle_binding("--vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn three_fns_middle_binding_cranelift() {
    check_three_fns_middle_binding("--jit");
}
