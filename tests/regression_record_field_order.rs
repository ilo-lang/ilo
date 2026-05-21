// Regression tests for #5bg — cross-engine field-order variance in record
// Display output.
//
// Background: `Value::Record` stores fields in a `HashMap<String, Value>`.
// The `impl Display for Value` arm previously iterated `fields` in hashmap
// order, which is nondeterministic across runs (and between engines, since
// VM/JIT route through the same Value::Record but populate it via different
// code paths). The result: `prnt` of a record literal could print
// `r {a: 2, b: 1, c: 3}` on tree and `r {b: 1, a: 2, c: 3}` on the VM,
// breaking agents that diff stdout across engines for self-verification.
//
// `jdmp` already canonicalises keys for record JSON output, so this fix
// aligns Display with jdmp: sort field names lexicographically before
// rendering.
//
// All three engines (tree, VM, Cranelift JIT) and both Display paths
// (`prnt` direct, `fmt "{}"` formatter) are covered below.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine_arg: Option<&str>, src: &str, entry: &str) -> String {
    let mut cmd = ilo();
    cmd.arg(src);
    if let Some(e) = engine_arg {
        cmd.arg(e);
    }
    cmd.arg(entry);
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine_arg:?} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// Record declared in non-sorted order (b, a, c). Display must always
// produce a, b, c regardless of engine or Display path.
const PRNT_SRC: &str = "type r{b:n;a:n;c:n}\ngo>_;v=r b:1 a:2 c:3;prnt v\n";
const FMT_SRC: &str = "type r{b:n;a:n;c:n}\ngo>t;v=r b:1 a:2 c:3;fmt \"{}\" v\n";
const EXPECTED: &str = "r {a: 2, b: 1, c: 3}";

// ── prnt direct ──────────────────────────────────────────────────────────

#[test]
fn prnt_record_lex_sorted_tree() {
    assert_eq!(run(None, PRNT_SRC, "go"), EXPECTED);
}

#[test]
fn prnt_record_lex_sorted_vm() {
    assert_eq!(run(Some("--vm"), PRNT_SRC, "go"), EXPECTED);
}

#[cfg(feature = "cranelift")]
#[test]
fn prnt_record_lex_sorted_jit() {
    assert_eq!(run(Some("--jit"), PRNT_SRC, "go"), EXPECTED);
}

// ── fmt "{}" formatter ────────────────────────────────────────────────────

#[test]
fn fmt_record_lex_sorted_tree() {
    assert_eq!(run(None, FMT_SRC, "go"), EXPECTED);
}

#[test]
fn fmt_record_lex_sorted_vm() {
    assert_eq!(run(Some("--vm"), FMT_SRC, "go"), EXPECTED);
}

#[cfg(feature = "cranelift")]
#[test]
fn fmt_record_lex_sorted_jit() {
    assert_eq!(run(Some("--jit"), FMT_SRC, "go"), EXPECTED);
}

// Cross-engine parity: same source on all engines produces the same bytes.
#[test]
fn record_display_cross_engine_parity() {
    let tree = run(None, PRNT_SRC, "go");
    let vm = run(Some("--vm"), PRNT_SRC, "go");
    assert_eq!(tree, vm, "tree vs vm diverged");
    #[cfg(feature = "cranelift")]
    {
        let jit = run(Some("--jit"), PRNT_SRC, "go");
        assert_eq!(tree, jit, "tree vs jit diverged");
    }
}
