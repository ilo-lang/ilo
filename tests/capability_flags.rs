/// Integration tests for CLI capability flags (`--allow-net`, `--allow-read`,
/// `--allow-write`, `--allow-run`).
///
/// Each test runs programs through `interpreter::run_with_caps` and
/// `vm::run_with_caps` to verify enforcement at both backends, then also
/// exercises the `Caps` unit helpers directly for clarity.
use ilo::caps::{Caps, Policy};
use ilo::interpreter::{self, Value};
use ilo::{lexer, parser, vm};
use std::sync::Arc;

// ── helpers ───────────────────────────────────────────────────────────────────

fn make_program(src: &str) -> ilo::ast::Program {
    let tokens = lexer::lex(src).unwrap();
    let token_spans: Vec<(ilo::lexer::Token, ilo::ast::Span)> = tokens
        .into_iter()
        .map(|(t, r)| {
            (
                t,
                ilo::ast::Span {
                    start: r.start,
                    end: r.end,
                },
            )
        })
        .collect();
    let (mut program, _) = parser::parse(token_spans);
    ilo::ast::resolve_aliases(&mut program);
    ilo::ast::desugar_dot_var_index(&mut program);
    program
}

/// Run a 0-arg function through interpreter + VM with given caps.
/// Returns the result value for both backends.
fn run_both(src: &str, caps: Caps) -> (Value, Value) {
    let caps = Arc::new(caps);
    let program = make_program(src);
    let tree_result =
        interpreter::run_with_caps(&program, None, vec![], Arc::clone(&caps)).unwrap();
    let compiled = vm::compile(&program).unwrap();
    let vm_result = vm::run_with_caps(&compiled, None, vec![], caps).unwrap();
    (tree_result, vm_result)
}

fn is_err_value(v: &Value) -> bool {
    matches!(v, Value::Err(_))
}

fn err_text(v: &Value) -> String {
    match v {
        Value::Err(inner) => match inner.as_ref() {
            Value::Text(s) => s.to_string(),
            other => format!("{other:?}"),
        },
        other => panic!("expected Err value, got {other:?}"),
    }
}

// ── default permissive mode ───────────────────────────────────────────────────

#[test]
fn default_caps_does_not_block_file_read() {
    // Write a temp file and read it — should succeed with no caps flags.
    let path = "/tmp/ilo_cap_test_read.txt";
    std::fs::write(path, "hello caps").unwrap();
    let src = format!("f>R t t;rd \"{path}\"");
    let (tree, vm) = run_both(&src, Caps::default());
    // With permissive caps, rd returns Ok(text) — not Err.
    assert!(
        !is_err_value(&tree),
        "tree: unexpected Err with default caps"
    );
    assert!(!is_err_value(&vm), "vm: unexpected Err with default caps");
}

// ── --allow-net: empty list blocks all network ────────────────────────────────

/// Core test from the task brief: `--allow-net=` (empty list) + `get` → Err.
/// Network call is blocked before any HTTP request is issued.
#[test]
fn allow_net_empty_blocks_get() {
    let caps = Caps::Restricted {
        net: Policy::List(vec![]),
        read: Policy::All,
        write: Policy::All,
        run: Policy::All,
        env: Policy::All,
    };
    let src = "f>R t t;get \"https://example.com\"";
    let (tree, vm_val) = run_both(src, caps);
    assert!(is_err_value(&tree), "tree: expected Err, got {tree:?}");
    assert!(is_err_value(&vm_val), "vm: expected Err, got {vm_val:?}");
    let msg = err_text(&tree);
    assert!(
        msg.contains("ILO-CAP-001"),
        "err should include ILO-CAP-001 code, got: {msg}"
    );
    assert!(
        msg.contains("--allow-net"),
        "err should mention --allow-net, got: {msg}"
    );
    assert!(
        msg.contains("example.com"),
        "err should mention host, got: {msg}"
    );
}

#[test]
fn allow_net_empty_blocks_get_vm_message() {
    let caps = Caps::Restricted {
        net: Policy::List(vec![]),
        read: Policy::All,
        write: Policy::All,
        run: Policy::All,
        env: Policy::All,
    };
    let src = "f>R t t;get \"https://example.com\"";
    let program = make_program(src);
    let compiled = vm::compile(&program).unwrap();
    let result = vm::run_with_caps(&compiled, None, vec![], Arc::new(caps)).unwrap();
    let msg = err_text(&result);
    assert!(
        msg.contains("--allow-net"),
        "vm err should mention --allow-net; got: {msg}"
    );
}

// ── --allow-net: specific host allowlist ─────────────────────────────────────

#[test]
fn allow_net_blocks_non_allowlisted_host() {
    let caps = Caps::Restricted {
        net: Policy::List(vec!["allowed.example".to_owned()]),
        read: Policy::All,
        write: Policy::All,
        run: Policy::All,
        env: Policy::All,
    };
    let src = "f>R t t;get \"https://evil.example\"";
    let (tree, vm_val) = run_both(src, caps);
    assert!(
        is_err_value(&tree),
        "tree: expected Err for non-allowlisted host"
    );
    assert!(
        is_err_value(&vm_val),
        "vm: expected Err for non-allowlisted host"
    );
}

// ── --allow-read: prefix enforcement ─────────────────────────────────────────

/// Core test from the task brief: `--allow-read=/tmp` + `rd "/etc/passwd"` → Err.
#[test]
fn allow_read_blocks_outside_prefix() {
    let caps = Caps::Restricted {
        net: Policy::All,
        read: Policy::List(vec!["/tmp".to_owned()]),
        write: Policy::All,
        run: Policy::All,
        env: Policy::All,
    };
    let src = "f>R t t;rd \"/etc/passwd\"";
    let (tree, vm_val) = run_both(src, caps);
    assert!(
        is_err_value(&tree),
        "tree: expected Err for /etc/passwd when read limited to /tmp"
    );
    assert!(
        is_err_value(&vm_val),
        "vm: expected Err for /etc/passwd when read limited to /tmp"
    );
    let msg = err_text(&tree);
    assert!(
        msg.contains("ILO-CAP-001"),
        "err should include ILO-CAP-001 code, got: {msg}"
    );
    assert!(
        msg.contains("--allow-read"),
        "err should mention --allow-read, got: {msg}"
    );
}

#[test]
fn allow_read_permits_inside_prefix() {
    let path = "/tmp/ilo_cap_test_read2.txt";
    std::fs::write(path, "ok").unwrap();
    let caps = Caps::Restricted {
        net: Policy::All,
        read: Policy::List(vec!["/tmp".to_owned()]),
        write: Policy::All,
        run: Policy::All,
        env: Policy::All,
    };
    let src = format!("f>R t t;rd \"{path}\"");
    let (tree, vm_val) = run_both(&src, caps);
    // /tmp is in the allowlist so rd should succeed (returns Ok, not Err).
    assert!(
        !is_err_value(&tree),
        "tree: /tmp should be permitted, got {tree:?}"
    );
    assert!(
        !is_err_value(&vm_val),
        "vm: /tmp should be permitted, got {vm_val:?}"
    );
}

// ── --allow-write: enforcement ────────────────────────────────────────────────

#[test]
fn allow_write_blocks_outside_prefix() {
    let caps = Caps::Restricted {
        net: Policy::All,
        read: Policy::All,
        write: Policy::List(vec!["/tmp/ilo_allowed".to_owned()]),
        run: Policy::All,
        env: Policy::All,
    };
    let src = "f>R t t;wr \"/etc/evil.txt\" \"data\"";
    let (tree, vm_val) = run_both(src, caps);
    assert!(
        is_err_value(&tree),
        "tree: expected Err for write outside prefix"
    );
    assert!(
        is_err_value(&vm_val),
        "vm: expected Err for write outside prefix"
    );
    let msg = err_text(&tree);
    assert!(
        msg.contains("ILO-CAP-001"),
        "err should include ILO-CAP-001 code, got: {msg}"
    );
    assert!(
        msg.contains("--allow-write"),
        "err should mention --allow-write, got: {msg}"
    );
}

// ── --allow-run: enforcement ──────────────────────────────────────────────────

#[test]
fn allow_run_empty_blocks_run() {
    let caps = Caps::Restricted {
        net: Policy::All,
        read: Policy::All,
        write: Policy::All,
        run: Policy::List(vec![]),
        env: Policy::All,
    };
    let src = "f>R (M t t) t;run \"echo\" [\"hello\"]";
    // Only test via tree interpreter (run goes through tree-bridge in VM).
    let program = make_program(src);
    let result = interpreter::run_with_caps(&program, None, vec![], Arc::new(caps)).unwrap();
    assert!(
        is_err_value(&result),
        "expected Err when run allowlist is empty, got {result:?}"
    );
    let msg = err_text(&result);
    assert!(
        msg.contains("ILO-CAP-001"),
        "err should include ILO-CAP-001 code, got: {msg}"
    );
    assert!(
        msg.contains("--allow-run"),
        "err should mention --allow-run, got: {msg}"
    );
}

#[test]
fn allow_run_permits_allowlisted_cmd() {
    let caps = Caps::Restricted {
        net: Policy::All,
        read: Policy::All,
        write: Policy::All,
        run: Policy::List(vec!["echo".to_owned()]),
        env: Policy::All,
    };
    let src = "f>R (M t t) t;run \"echo\" [\"hello\"]";
    let program = make_program(src);
    let result = interpreter::run_with_caps(&program, None, vec![], Arc::new(caps)).unwrap();
    // echo is in the allowlist — should return Ok(...), not Err.
    assert!(
        !is_err_value(&result),
        "echo should be permitted, got {result:?}"
    );
}

// ── No --allow-* flags → permissive (backwards compat) ───────────────────────

#[test]
fn no_allow_flags_is_permissive_for_file_read() {
    let path = "/tmp/ilo_cap_permissive.txt";
    std::fs::write(path, "permissive").unwrap();
    let src = format!("f>R t t;rd \"{path}\"");
    let (tree, vm_val) = run_both(&src, Caps::Permissive);
    assert!(
        !is_err_value(&tree),
        "tree: Permissive caps should allow rd, got {tree:?}"
    );
    assert!(
        !is_err_value(&vm_val),
        "vm: Permissive caps should allow rd, got {vm_val:?}"
    );
}

// ── `Caps::parse_allow` round-trip ───────────────────────────────────────────

#[test]
fn parse_allow_star_is_all() {
    assert_eq!(Caps::parse_allow("*"), Policy::All);
}

#[test]
fn parse_allow_empty_is_empty_list() {
    assert_eq!(Caps::parse_allow(""), Policy::List(vec![]));
}

#[test]
fn parse_allow_comma_list() {
    assert_eq!(
        Caps::parse_allow("a.com,b.com"),
        Policy::List(vec!["a.com".to_owned(), "b.com".to_owned()])
    );
}

// ── `build_caps` semantics through Caps API ───────────────────────────────────

/// When no --allow-* flags are present, caps is Permissive (no restriction).
#[test]
fn permissive_caps_allow_everything() {
    let caps = Caps::Permissive;
    assert!(caps.check_net("https://any.host").is_ok());
    assert!(caps.check_read("/etc/passwd").is_ok());
    assert!(caps.check_write("/etc/passwd").is_ok());
    assert!(caps.check_run("rm").is_ok());
}

// ── --allow-read: fs-metadata builtins (fsize/mtime/isfile/isdir) ─────────────

/// `fsize` blocked by --allow-read → returns Err containing ILO-CAP-001.
#[test]
fn allow_read_blocks_fsize() {
    let path = "/tmp/ilo_cap_fsize.txt";
    std::fs::write(path, "hello").unwrap();
    let caps = Caps::Restricted {
        net: Policy::All,
        read: Policy::List(vec![]),
        write: Policy::All,
        run: Policy::All,
        env: Policy::All,
    };
    let src = format!("f>R n t;fsize \"{path}\"");
    let (tree, _vm_val) = run_both(&src, caps);
    assert!(
        is_err_value(&tree),
        "tree: fsize should be blocked by read cap"
    );
    let msg = err_text(&tree);
    assert!(
        msg.contains("ILO-CAP-001"),
        "expected ILO-CAP-001, got: {msg}"
    );
}

/// `mtime` blocked by --allow-read → returns Err containing ILO-CAP-001.
#[test]
fn allow_read_blocks_mtime() {
    let path = "/tmp/ilo_cap_mtime.txt";
    std::fs::write(path, "hello").unwrap();
    let caps = Caps::Restricted {
        net: Policy::All,
        read: Policy::List(vec![]),
        write: Policy::All,
        run: Policy::All,
        env: Policy::All,
    };
    let src = format!("f>R n t;mtime \"{path}\"");
    let (tree, _vm_val) = run_both(&src, caps);
    assert!(
        is_err_value(&tree),
        "tree: mtime should be blocked by read cap"
    );
    let msg = err_text(&tree);
    assert!(
        msg.contains("ILO-CAP-001"),
        "expected ILO-CAP-001, got: {msg}"
    );
}

/// `isfile` blocked by --allow-read → returns false (collapses to bool false).
#[test]
fn allow_read_blocks_isfile_returns_false() {
    let path = "/tmp/ilo_cap_isfile.txt";
    std::fs::write(path, "hello").unwrap();
    let caps = Caps::Restricted {
        net: Policy::All,
        read: Policy::List(vec![]),
        write: Policy::All,
        run: Policy::All,
        env: Policy::All,
    };
    let src = format!("f>b;isfile \"{path}\"");
    let (tree, _vm_val) = run_both(&src, caps);
    assert_eq!(
        tree,
        Value::Bool(false),
        "isfile should return false when blocked by read cap"
    );
}

/// `isdir` blocked by --allow-read → returns false.
#[test]
fn allow_read_blocks_isdir_returns_false() {
    let caps = Caps::Restricted {
        net: Policy::All,
        read: Policy::List(vec![]),
        write: Policy::All,
        run: Policy::All,
        env: Policy::All,
    };
    let src = "f>b;isdir \"/tmp\"";
    let (tree, _vm_val) = run_both(src, caps);
    assert_eq!(
        tree,
        Value::Bool(false),
        "isdir should return false when blocked by read cap"
    );
}

/// When any --allow-* flag is set, undefined dimensions default to Policy::All.
#[test]
fn one_flag_restricts_only_that_dimension() {
    let caps = Caps::Restricted {
        net: Policy::List(vec![]), // net blocked
        read: Policy::All,         // read unrestricted
        write: Policy::All,        // write unrestricted
        run: Policy::All,          // run unrestricted
        env: Policy::All,          // env unrestricted
    };
    assert!(
        caps.check_net("https://any.host").is_err(),
        "net should be blocked"
    );
    assert!(
        caps.check_read("/etc/passwd").is_ok(),
        "read should be unrestricted"
    );
    assert!(
        caps.check_write("/tmp/x").is_ok(),
        "write should be unrestricted"
    );
    assert!(caps.check_run("rm").is_ok(), "run should be unrestricted");
}
