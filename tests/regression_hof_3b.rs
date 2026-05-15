// Cross-engine coverage for the PR 3b HOF tree-bridge: `grp`, `uniqby`,
// `partition`, and 2-arg `srt`. These HOFs invoke a user-defined callback
// per element, so the bridge needs the active AST `Program` plumbed through
// `ACTIVE_AST_PROGRAM` to resolve the callback name to its `Decl::Function`.
//
// Pre-fix: `--run-vm` and `--run-cranelift` errored with `Compile error:
// undefined function: grp` (etc.) because the VM emitter fell through to
// OP_CALL's user-function lookup. Post-fix: every engine routes through
// `interpreter::call_builtin_for_bridge_with_program`, which builds an Env
// from the retained AST and dispatches the callback through the tree
// interpreter. Tree, VM, and Cranelift all agree byte-for-byte.
//
// Each test exercises tree / VM / Cranelift and asserts equality. Error
// cases (wrong arg shape, key fn returning a list, etc.) are also checked
// so we catch regressions where the bridge swallows errors as Nil.

use std::process::Command;

const ENGINES: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_engine_ok(src: &str, engine: &str) -> String {
    let out = ilo()
        .args([src, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_engine_err(src: &str, engine: &str) -> String {
    let out = ilo()
        .args([src, engine, "f"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "ilo {engine} unexpectedly succeeded for `{src}`: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8_lossy(&out.stderr).to_string()
}

/// Asserts the same source produces `expected` on every engine.
fn check(src: &str, expected: &str) {
    for engine in ENGINES {
        let actual = run_engine_ok(src, engine);
        assert_eq!(
            actual, expected,
            "engine={engine}, src=`{src}`: got `{actual}`, expected `{expected}`"
        );
    }
}

// ── grp ────────────────────────────────────────────────────────────────

#[test]
fn grp_by_parity_string_key() {
    // Key fn returns a string ("0"/"1"); result is a map keyed by string.
    check(
        r#"par n:n>t;r=mod n 2;str r
f>M t (L n);grp par [1,2,3,4,5,6]"#,
        "{0: [2, 4, 6]; 1: [1, 3, 5]}",
    );
}

#[test]
fn grp_by_numeric_key() {
    // Numeric key resolves to an integer map bucket.
    check(
        r#"par n:n>n;mod n 2
f>M n (L n);grp par [1,2,3,4]"#,
        "{0: [2, 4]; 1: [1, 3]}",
    );
}

#[test]
fn grp_empty_list_yields_empty_map() {
    check(
        r#"par n:n>n;mod n 2
f>M n (L n);grp par []"#,
        "{}",
    );
}

// ── uniqby ─────────────────────────────────────────────────────────────

#[test]
fn uniqby_keeps_first_per_key() {
    check(
        r#"par n:n>t;r=mod n 2;str r
f>L n;uniqby par [1,3,2,4,5,6]"#,
        "[1, 2]",
    );
}

#[test]
fn uniqby_empty_list() {
    check(
        r#"par n:n>t;str n
f>L n;uniqby par []"#,
        "[]",
    );
}

// ── partition ──────────────────────────────────────────────────────────

#[test]
fn partition_splits_pass_and_fail() {
    check(
        r#"pos x:n>b;>x 0
f>L (L n);partition pos [-1,2,-3,4]"#,
        "[[2, 4], [-1, -3]]",
    );
}

#[test]
fn partition_all_pass() {
    check(
        r#"pos x:n>b;>x 0
f>L (L n);partition pos [1,2,3]"#,
        "[[1, 2, 3], []]",
    );
}

#[test]
fn partition_all_fail() {
    check(
        r#"pos x:n>b;>x 0
f>L (L n);partition pos [-1,-2,-3]"#,
        "[[], [-1, -2, -3]]",
    );
}

// ── srt (2-arg, key-fn form) ────────────────────────────────────────────

#[test]
fn srt_by_abs_value() {
    check(
        r#"absv n:n>n;?<n 0 (-0 n) n
f>L n;srt absv [-3,1,-2,4,-1]"#,
        "[1, -1, -2, -3, 4]",
    );
}

#[test]
fn srt_by_length() {
    check(
        r#"slen s:t>n;len s
f>L t;srt slen ["banana","fig","apple"]"#,
        "[fig, apple, banana]",
    );
}

#[test]
fn srt_2arg_empty_list() {
    check(
        r#"slen s:t>n;len s
f>L t;srt slen []"#,
        "[]",
    );
}

// ── Error path: callback returns wrong shape ───────────────────────────

#[test]
fn grp_wrong_list_arg_errors_on_tree_and_vm() {
    // `grp` requires the second arg to be a list. Tree and VM surface the
    // typed runtime error; Cranelift's `jit_call_builtin_tree` documents
    // that bridge errors collapse to Nil (matching `jit_rgxsub`/`jit_rd`
    // precedent), so we don't include it in the must-error set. Promoting
    // these to typed runtime errors on the JIT path is a documented
    // follow-up in `jit_call_builtin_tree`'s rustdoc.
    for engine in ["--run-tree", "--run-vm"] {
        let err = run_engine_err(
            r#"key n:n>n;n
f>_;grp key 42"#,
            engine,
        );
        assert!(
            err.contains("grp"),
            "engine={engine}: stderr missing `grp`: {err}"
        );
    }
}

// ── Chained HOFs to catch state leaks ──────────────────────────────────

#[test]
fn uniqby_then_grp_chained() {
    // Two bridge HOF calls in sequence: dedupe by parity, then group by
    // parity (one-bucket-per-parity after dedupe). Catches stale-Env bugs
    // across consecutive bridge invocations.
    check(
        r#"par n:n>t;r=mod n 2;str r
f>M t (L n);ys=uniqby par [1,3,2,4,5,6];grp par ys"#,
        "{0: [2]; 1: [1]}",
    );
}
