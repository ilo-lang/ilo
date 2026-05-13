// Regression tests for `_` wildcard binding in match arms.
//
// Background:
//
// Before this fix, `_` in a pattern position (Pattern::Wildcard, Pattern::Ok("_"),
// Pattern::Err("_"), Pattern::TypeIs { binding: "_" }) was treated as "no
// binding" — the verifier, tree interpreter, VM compiler and Python codegen
// all skipped inserting `_` into the local scope. Bodies that referenced `_`
// failed with `undefined variable '_'` (ILO-T004), even though SPEC.md line
// 1069's documented example uses `~_:~_` — a wildcard-ok arm that re-wraps
// the unchanged inner value via `_` in the body.
//
// The fix:
//
// `_` is now bound like any other name. For Pattern::Ok/Err it binds to the
// unwrapped inner value; for Pattern::TypeIs it binds to the matched value;
// for Pattern::Wildcard it binds to the match subject itself. Bodies that
// never reference `_` are unaffected (they just don't read the local) so this
// is a strict superset of the previous behaviour.
//
// All tests cross-engine (tree, VM, Cranelift) so a divergence between
// backends shows up in CI.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run(engine: &str, src: &str, entry: &str, arg: &str) -> String {
    let mut cmd = ilo();
    if arg.is_empty() {
        cmd.args([src, engine, entry]);
    } else {
        cmd.args([src, engine, entry, arg]);
    }
    let out = cmd.output().expect("failed to run ilo");
    // Top-level Err values exit non-zero with `^...` on stderr (per #255 and
    // the ok-wrapper-stdout follow-up). Accept that as a non-failure here so
    // the wildcard tests can assert against the rendered Result regardless of
    // which arm fires.
    if out.status.success() {
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if stderr.starts_with('^') {
            stderr
        } else {
            panic!(
                "ilo {engine} failed for `{src}`: stderr={}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}

// ── `^_:nil` discard (body never references `_`) — must still work ──────────
//
// Negative regression: the existing wildcard-as-discard idiom must keep
// working. This is the exact shape from the assessment doc's resolved-section
// example (`?r{~v:v;^_:0}`).

const WILDCARD_DISCARD: &str = "pn s:t>n;r=num s;?r{~v:v;^_:0}\n";

#[test]
fn wildcard_discard_ok_tree() {
    assert_eq!(run("--run-tree", WILDCARD_DISCARD, "pn", "3.14"), "3.14");
}

#[test]
fn wildcard_discard_ok_vm() {
    assert_eq!(run("--run-vm", WILDCARD_DISCARD, "pn", "3.14"), "3.14");
}

#[test]
#[cfg(feature = "cranelift")]
fn wildcard_discard_ok_cranelift() {
    assert_eq!(
        run("--run-cranelift", WILDCARD_DISCARD, "pn", "3.14"),
        "3.14"
    );
}

#[test]
fn wildcard_discard_err_tree() {
    assert_eq!(run("--run-tree", WILDCARD_DISCARD, "pn", "oops"), "0");
}

#[test]
fn wildcard_discard_err_vm() {
    assert_eq!(run("--run-vm", WILDCARD_DISCARD, "pn", "oops"), "0");
}

#[test]
#[cfg(feature = "cranelift")]
fn wildcard_discard_err_cranelift() {
    assert_eq!(run("--run-cranelift", WILDCARD_DISCARD, "pn", "oops"), "0");
}

// ── `~_:~_` re-wrap unchanged (SPEC.md line 1069) ───────────────────────────
//
// The headline case: a wildcard-ok arm references `_` in the body to
// re-construct the same Ok value. Returns Result so we can check both arms.

const REWRAP_UNCHANGED: &str = "f s:t>R n t;r=num s;?r{~_:~_;^_:^\"e\"}\n";

#[test]
fn rewrap_unchanged_ok_tree() {
    assert_eq!(run("--run-tree", REWRAP_UNCHANGED, "f", "3.14"), "~3.14");
}

#[test]
fn rewrap_unchanged_ok_vm() {
    assert_eq!(run("--run-vm", REWRAP_UNCHANGED, "f", "3.14"), "~3.14");
}

#[test]
#[cfg(feature = "cranelift")]
fn rewrap_unchanged_ok_cranelift() {
    assert_eq!(
        run("--run-cranelift", REWRAP_UNCHANGED, "f", "3.14"),
        "~3.14"
    );
}

#[test]
fn rewrap_unchanged_err_tree() {
    assert_eq!(run("--run-tree", REWRAP_UNCHANGED, "f", "oops"), "^e");
}

#[test]
fn rewrap_unchanged_err_vm() {
    assert_eq!(run("--run-vm", REWRAP_UNCHANGED, "f", "oops"), "^e");
}

#[test]
#[cfg(feature = "cranelift")]
fn rewrap_unchanged_err_cranelift() {
    assert_eq!(run("--run-cranelift", REWRAP_UNCHANGED, "f", "oops"), "^e");
}

// ── `^_:fmt "err: {}" _` — debug-log the unbound name ───────────────────────
//
// `_` in an err-wildcard arm now refers to the inner error text, so a
// throwaway debug formatter composes without renaming to `^e:fmt ... e`.

const ERR_DEBUG_FMT: &str = "f s:t>t;r=num s;?r{~v:str v;^_:fmt \"err: {}\" _}\n";

#[test]
fn err_debug_fmt_tree() {
    assert_eq!(run("--run-tree", ERR_DEBUG_FMT, "f", "abc"), "err: abc");
}

#[test]
fn err_debug_fmt_vm() {
    assert_eq!(run("--run-vm", ERR_DEBUG_FMT, "f", "abc"), "err: abc");
}

#[test]
#[cfg(feature = "cranelift")]
fn err_debug_fmt_cranelift() {
    assert_eq!(
        run("--run-cranelift", ERR_DEBUG_FMT, "f", "abc"),
        "err: abc"
    );
}

// ── `_:_` plain wildcard binds the subject ──────────────────────────────────
//
// Outside result-match, `_:body` is the universal catch-all. With binding,
// `_` in the body resolves to the matched subject — useful for default arms
// that want to echo the input.

const PLAIN_WILDCARD_BIND: &str = "f x:n>n;?x{1:10;_:_}\n";

#[test]
fn plain_wildcard_bind_subject_tree() {
    assert_eq!(run("--run-tree", PLAIN_WILDCARD_BIND, "f", "42"), "42");
}

#[test]
fn plain_wildcard_bind_subject_vm() {
    assert_eq!(run("--run-vm", PLAIN_WILDCARD_BIND, "f", "42"), "42");
}

#[test]
#[cfg(feature = "cranelift")]
fn plain_wildcard_bind_subject_cranelift() {
    assert_eq!(run("--run-cranelift", PLAIN_WILDCARD_BIND, "f", "42"), "42");
}

// Hit-the-literal arm: confirms the wildcard fall-through still picks up
// non-matching inputs.

#[test]
fn plain_wildcard_bind_literal_tree() {
    assert_eq!(run("--run-tree", PLAIN_WILDCARD_BIND, "f", "1"), "10");
}

#[test]
fn plain_wildcard_bind_literal_vm() {
    assert_eq!(run("--run-vm", PLAIN_WILDCARD_BIND, "f", "1"), "10");
}

#[test]
#[cfg(feature = "cranelift")]
fn plain_wildcard_bind_literal_cranelift() {
    assert_eq!(run("--run-cranelift", PLAIN_WILDCARD_BIND, "f", "1"), "10");
}

// ── `n _:_` TypeIs wildcard binds the typed subject ─────────────────────────
//
// TypeIs with `_` binding should also expose the matched value via `_`.

const TYPEIS_WILDCARD_BIND: &str = "f x:n>n;?x{n _:+_ 1;_:0}\n";

#[test]
fn typeis_wildcard_bind_tree() {
    assert_eq!(run("--run-tree", TYPEIS_WILDCARD_BIND, "f", "5"), "6");
}

#[test]
fn typeis_wildcard_bind_vm() {
    assert_eq!(run("--run-vm", TYPEIS_WILDCARD_BIND, "f", "5"), "6");
}

#[test]
#[cfg(feature = "cranelift")]
fn typeis_wildcard_bind_cranelift() {
    assert_eq!(run("--run-cranelift", TYPEIS_WILDCARD_BIND, "f", "5"), "6");
}

// ── Negative regression: named bindings still work ──────────────────────────
//
// The fix removed the `binding != "_"` guard everywhere; named bindings
// (the common case) must continue to work end-to-end.

const NAMED_BINDINGS: &str = "f s:t>t;r=num s;?r{~v:str v;^e:+\"err: \" e}\n";

#[test]
fn named_bindings_ok_tree() {
    assert_eq!(run("--run-tree", NAMED_BINDINGS, "f", "3.14"), "3.14");
}

#[test]
fn named_bindings_ok_vm() {
    assert_eq!(run("--run-vm", NAMED_BINDINGS, "f", "3.14"), "3.14");
}

#[test]
#[cfg(feature = "cranelift")]
fn named_bindings_ok_cranelift() {
    assert_eq!(run("--run-cranelift", NAMED_BINDINGS, "f", "3.14"), "3.14");
}

#[test]
fn named_bindings_err_tree() {
    assert_eq!(run("--run-tree", NAMED_BINDINGS, "f", "abc"), "err: abc");
}

#[test]
fn named_bindings_err_vm() {
    assert_eq!(run("--run-vm", NAMED_BINDINGS, "f", "abc"), "err: abc");
}

#[test]
#[cfg(feature = "cranelift")]
fn named_bindings_err_cranelift() {
    assert_eq!(
        run("--run-cranelift", NAMED_BINDINGS, "f", "abc"),
        "err: abc"
    );
}
