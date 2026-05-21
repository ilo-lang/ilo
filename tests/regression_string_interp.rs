// Regression tests for `{name}` string interpolation.
//
// Manifesto principle 1 (token-conservative): `"hello {name}"` is cheaper
// for an agent to write than `fmt "hello {}" name`. The parser desugars
// `{ident}` slots inside double-quoted strings into a `fmt` call at parse
// time so every engine (tree / VM / Cranelift) sees the same AST shape.
//
// Scope pinned by these tests:
//   - Single `{name}` slot resolves to the corresponding binding.
//   - Multiple `{name}` slots resolve left-to-right.
//   - `{{` / `}}` collapse to literal `{` / `}` inside interpolated strings.
//   - Bare `{}` (today's positional placeholder) keeps its existing meaning.
//   - Mixing `{ident}` and `{}` in the same string is left verbatim: we
//     don't claim more surface area than scope.
//   - `{nonexistent}` errors via the underlying fmt arg-resolution path
//     (ILO-T004 undefined-variable), not silently.
//   - Hyphenated idents like `{a-b}` work because the lexer's ident regex
//     allows them.
//   - Plain non-interpolated strings continue to round-trip unchanged.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn write_src(name: &str, src: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "ilo_strinterp_{name}_{}_{n}.ilo",
        std::process::id()
    ));
    std::fs::write(&path, src).expect("write src");
    path
}

fn run_ok(engine: &str, src: &str, entry: &str, args: &[&str]) -> String {
    let path = write_src(entry, src);
    let mut cmd = ilo();
    cmd.arg(&path).arg(engine).arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        out.status.success(),
        "ilo {engine} failed for `{src}`: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn run_err(engine: &str, src: &str, entry: &str, args: &[&str]) -> String {
    let path = write_src(entry, src);
    let mut cmd = ilo();
    cmd.arg(&path).arg(engine).arg(entry);
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().expect("failed to run ilo");
    let _ = std::fs::remove_file(&path);
    assert!(
        !out.status.success(),
        "expected failure but ilo {engine} succeeded for `{src}`"
    );
    let mut s = String::from_utf8_lossy(&out.stderr).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stdout));
    s
}

// Engines to fan every assertion across. `--vm` runs the tree-bridge for
// 1-arg builtins like fmt unless the dispatch table opts into the native
// VM op; `--jit` exercises the Cranelift backend.
const ENGINES: &[&str] = {
    #[cfg(feature = "cranelift")]
    {
        &["--vm", "--jit"]
    }
    #[cfg(not(feature = "cranelift"))]
    {
        &["--vm"]
    }
};

// ── 1) Single `{name}` slot ────────────────────────────────────────────────

#[test]
fn single_ident_slot() {
    let src = "f name:t>t;fmt \"hello {name}\"";
    for engine in ENGINES {
        assert_eq!(
            run_ok(engine, src, "f", &["world"]),
            "hello world",
            "engine={engine}"
        );
    }
}

// ── 2) Multiple `{name}` slots resolve left-to-right ───────────────────────

#[test]
fn multiple_ident_slots() {
    let src = "f a:t b:t>t;fmt \"{a} and {b}\"";
    for engine in ENGINES {
        assert_eq!(
            run_ok(engine, src, "f", &["hi", "there"]),
            "hi and there",
            "engine={engine}"
        );
    }
}

// ── 3) `{{` escapes to literal `{`, `}}` to literal `}` ────────────────────
//
// Only applies inside strings the desugar fires on (i.e. those with at
// least one `{ident}` slot). Other strings keep `{{`/`}}` verbatim so
// existing programs don't get silently rewritten.

#[test]
fn brace_escape_in_interpolated_string() {
    let src = "f name:t>t;fmt \"{{not}} {name}\"";
    for engine in ENGINES {
        assert_eq!(
            run_ok(engine, src, "f", &["world"]),
            "{not} world",
            "engine={engine}"
        );
    }
}

// ── 4) Bare `{}` positional placeholder still works ────────────────────────

#[test]
fn bare_placeholder_still_works() {
    let src = "f name:t>t;fmt \"hello {}\" name";
    for engine in ENGINES {
        assert_eq!(
            run_ok(engine, src, "f", &["world"]),
            "hello world",
            "engine={engine}"
        );
    }
}

// ── 5) Mixing `{ident}` and `{}` in the same string: left verbatim ─────────
//
// Scope rule: one style per string. If the agent writes
// `fmt "{name} {} done" other` we keep the template literal so the outer
// `fmt` call binds `other` to the first `{}` slot and the verifier surfaces
// the arity mismatch (slot count 2 vs 1 value arg) - not silent rewriting.

#[test]
fn mixed_ident_and_bare_left_verbatim() {
    // The desugar does not fire; `{name}` is treated as literal text and
    // the bare `{}` is filled by `other`.
    let src = "f name:t other:t>t;fmt \"{name} {} done\" other";
    for engine in ENGINES {
        assert_eq!(
            run_ok(engine, src, "f", &["alice", "bob"]),
            "{name} bob done",
            "engine={engine}"
        );
    }
}

// ── 6) Undefined-ident slot errors via ILO-T004 ────────────────────────────

#[test]
fn undefined_ident_in_slot_errors() {
    let src = "f>t;fmt \"{nonexistent}\"";
    for engine in ENGINES {
        let s = run_err(engine, src, "f", &[]);
        assert!(
            s.contains("ILO-T004") && s.contains("nonexistent"),
            "engine={engine}: expected ILO-T004 mentioning nonexistent, got: {s}"
        );
    }
}

// ── 7) Hyphenated idents work (lexer ident regex allows them) ──────────────

#[test]
fn hyphenated_ident_slot() {
    let src = "f a-b:t>t;fmt \"hi {a-b}\"";
    for engine in ENGINES {
        assert_eq!(
            run_ok(engine, src, "f", &["wow"]),
            "hi wow",
            "engine={engine}"
        );
    }
}

// ── 8) Plain non-interpolated strings unchanged ────────────────────────────

#[test]
fn plain_string_unchanged() {
    let src = "f>t;\"plain string\"";
    for engine in ENGINES {
        assert_eq!(
            run_ok(engine, src, "f", &[]),
            "plain string",
            "engine={engine}"
        );
    }
}

// ── 9) String with `{{` / `}}` but no `{ident}` keeps braces verbatim ──────
//
// Non-interpolated strings (no `{ident}` slot) are not touched by the
// desugar. This guards against silently changing semantics for programs
// that build JSON / config text containing literal `{{`/`}}` (rare but
// existed before this PR).

#[test]
fn double_brace_without_ident_unchanged() {
    let src = "f>t;\"{{bad}}\"";
    for engine in ENGINES {
        assert_eq!(run_ok(engine, src, "f", &[]), "{{bad}}", "engine={engine}");
    }
}

// ── 10) String with single `{tok}` where `tok` is not a valid ident ────────
//
// `{Foo}` (capital), `{x+1}` (operator), `{ }` (space) etc. should pass
// through verbatim because the lexer ident regex doesn't admit them.

#[test]
fn non_ident_brace_content_unchanged() {
    // Capital letter is not a valid ilo ident; the desugar leaves it alone.
    let src = "f>t;\"hello {World}\"";
    for engine in ENGINES {
        assert_eq!(
            run_ok(engine, src, "f", &[]),
            "hello {World}",
            "engine={engine}"
        );
    }
}
