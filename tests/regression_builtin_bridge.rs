// Regression coverage for the generic `OP_CALL_BUILTIN_TREE` bridge that
// lets tree-only builtins (rgx, rgxall, fmt variadic, 2-arg rd, rdb) run
// under `--run-vm` and `--run-cranelift` via interpreter fallback.
//
// Pre-fix, these all failed at VM compile time with
// `Compile error: undefined function: <name>` because the VM emitter fell
// through to OP_CALL's user-function lookup. The bridge routes them through
// the same `interpreter::call_function` the tree engine uses, so every
// engine produces identical output for the same source.
//
// Every test runs on tree, VM, and Cranelift, asserting all three engines
// agree on the result. That is the contract: future native lowerings can
// graduate any specific builtin off the bridge without changing user-visible
// behaviour.

use std::process::Command;

const ENGINES: &[&str] = &["--run-tree", "--run-vm", "--run-cranelift"];

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_engine(src: &str, engine: &str) -> String {
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

/// Asserts the same source produces `expected` on every engine.
fn check(src: &str, expected: &str) {
    for engine in ENGINES {
        let actual = run_engine(src, engine);
        assert_eq!(
            actual, expected,
            "engine={engine}, src=`{src}`: got `{actual}`, expected `{expected}`"
        );
    }
}

// ── rgx ────────────────────────────────────────────────────────────────

#[test]
fn rgx_no_group_returns_all_matches() {
    // Pre-fix: tree returned `[1, 2, 3]`; VM/Cranelift errored with
    // `Compile error: undefined function: rgx`.
    check(r#"f>L t;rgx "\d+" "a1 b2 c3""#, "[1, 2, 3]");
}

#[test]
fn rgx_with_group_returns_first_match_groups() {
    // With a capture group, `rgx` returns the groups from the first match
    // only (`rgxall` is the bulk variant). Same semantics on every engine.
    check(r#"f>L t;rgx "(\w+)=(\d+)" "x=1 y=22 z=333""#, "[x, 1]");
}

#[test]
fn rgx_no_match_returns_empty_list() {
    check(r#"f>L t;rgx "\d+" "no digits""#, "[]");
}

// ── rgxall ─────────────────────────────────────────────────────────────

#[test]
fn rgxall_html_extraction_cross_engine() {
    // The real-world HTML-scrape case that motivated `rgxall`. Cranelift
    // and VM must agree with tree byte-for-byte.
    check(
        r#"f>L (L t);rgxall "<h2>([^<]+)</h2>" "<h2>a</h2> <h2>b</h2> <h2>c</h2>""#,
        "[[a], [b], [c]]",
    );
}

#[test]
fn rgxall_two_groups_cross_engine() {
    check(
        r#"f>L (L t);rgxall "(\w+)=(\d+)" "x=1 y=22 z=333""#,
        "[[x, 1], [y, 22], [z, 333]]",
    );
}

// ── fmt (variadic) ─────────────────────────────────────────────────────

#[test]
fn fmt_zero_holes() {
    check(r#"f>t;fmt "literal""#, "literal");
}

#[test]
fn fmt_one_hole() {
    check(r#"f>t;fmt "x={}" 42"#, "x=42");
}

#[test]
fn fmt_three_holes_mixed_types() {
    // Variadic with a mix of number and text args — the case that
    // motivated the variadic shape over a fixed-arity opcode.
    check(r#"f>t;fmt "{} {} {}" 1 "two" 3"#, "1 two 3");
}

#[test]
fn fmt_extra_holes_left_as_literal() {
    // When the template has more `{}`s than args, the extra placeholders
    // pass through unchanged. Documented behaviour that the bridge must
    // preserve across engines.
    check(r#"f>t;fmt "{} {}" 1"#, "1 {}");
}

// ── rd (2-arg) ─────────────────────────────────────────────────────────

#[test]
fn rd_csv_two_arg_in_block_function() {
    // 2-arg `rd path fmt` returns `R (L (L t)) t`; auto-unwrap with `!`
    // requires the enclosing function to also return a Result.
    use std::io::Write;
    let mut path = std::env::temp_dir();
    path.push(format!("ilo-bridge-rd-{}.csv", std::process::id()));
    {
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, "a,1").unwrap();
        writeln!(f, "b,2").unwrap();
    }
    let src = format!(r#"f>R (L (L t)) t;rd "{}" "csv""#, path.to_str().unwrap());
    // Top-level Value::Ok prints bare (no `~` prefix) per the symmetric
    // stdout/stderr split — see regression_main_ok_stdout_bare.rs.
    check(&src, "[[a, 1], [b, 2]]");
    let _ = std::fs::remove_file(&path);
}

// ── rdb ────────────────────────────────────────────────────────────────

#[test]
fn rdb_csv_cross_engine() {
    // `rdb` parses an in-memory buffer in the given format; same dispatcher
    // as `rd`, so the bridge plumbing must handle both. Newline escapes
    // resolve at the string literal level.
    // Top-level Value::Ok prints bare (no `~` prefix).
    check(
        r#"f>R (L (L t)) t;rdb "a,1
b,2" "csv""#,
        "[[a, 1], [b, 2]]",
    );
}

#[test]
fn rdb_csv_with_bang_unwrap() {
    // Auto-unwrap `!` on a Result-returning bridge call: enclosing fn must
    // return Result, and `!` extracts the Ok inner. The wrap markers vanish
    // from the printed form because what's left is the inner list.
    check(
        r#"f>R (L (L t)) t;rdb! "a,1
b,2" "csv""#,
        "[[a, 1], [b, 2]]",
    );
}

// ── Cross-engine determinism guard ────────────────────────────────────

#[test]
fn engines_agree_on_chained_bridge_calls() {
    // Two bridge calls feeding each other: rgx output piped into fmt.
    // Catches state-leak bugs where a stale RC or NanVal tag survives
    // across consecutive bridge invocations.
    check(
        r#"f>t;hits=rgx "\d+" "a1 b2 c3";fmt "{} hits" len hits"#,
        "3 hits",
    );
}
