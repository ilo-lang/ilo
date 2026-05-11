// Regression tests for whitespace-separated bare references inside list
// literals. Previously `[a b c]` parsed as `[Call(a, [b, c])]` and failed
// verification with a cryptic "undefined function 'a' (called with 2 args)"
// while `[1 2 3]` and `[a,b,c]` both worked. Two agent personas tripped on
// the asymmetry, costing tokens on every retry. Inside list literals, bare
// idents are now parsed as list elements, mirroring the literal-only and
// comma-separated forms. Calls inside list elements still work via parens:
// `[(f x) y]` or `[f(x) y]`.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str) -> String {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

const BARE_REFS: &str = "f>L n;a=1;b=2;c=3;[a b c]";
const MIXED: &str = "f>L n;a=1;c=3;[a 2 c]";
const NUMERIC: &str = "f>L n;[1 2 3]";
const COMMA_REFS: &str = "f>L n;a=1;b=2;c=3;[a,b,c]";

fn check_all(engine: &str) {
    assert_eq!(
        run(engine, BARE_REFS, "f"),
        "[1, 2, 3]",
        "bare refs {engine}"
    );
    assert_eq!(run(engine, MIXED, "f"), "[1, 2, 3]", "mixed {engine}");
    assert_eq!(run(engine, NUMERIC, "f"), "[1, 2, 3]", "numeric {engine}");
    assert_eq!(run(engine, COMMA_REFS, "f"), "[1, 2, 3]", "comma {engine}");
    // `[f a]` resolves to a 2-element list of refs, NOT a call to f.
    // Decision: the list-literal form prioritises the "list of elements"
    // reading because it's the common case agents write; explicit parens
    // remain available when a call is intended.
    // Call form inside a list: use commas to separate elements so each
    // side parses as a full expression including whitespace-calls.
    // `[floor x, ceil x]` is the canonical form. The pure-whitespace form
    // `[floor x ceil x]` would yield four list elements, not two calls.
    let out = ilo()
        .args(["f x:n>L n;[floor x, ceil x]", engine, "f", "3.5"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} comma-call failed: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "[3, 4]",
        "comma-separated calls inside list {engine}"
    );
}

#[test]
fn list_literal_refs_tree() {
    check_all("--run-tree");
}

#[test]
fn list_literal_refs_vm() {
    check_all("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn list_literal_refs_cranelift() {
    check_all("--run-cranelift");
}
