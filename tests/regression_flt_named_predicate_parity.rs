// Cross-engine regression for ILO-472: `flt` with a named-helper
// predicate on a `L t`.
//
// Persona report (`hmac-cookie-parser`): the tree-walker engine
// returned the predicate's output (a bool) directly rather than the
// filtered list, while the VM produced the correct filtered list.
// Switching the named predicate to an inline lambda
// `flt (c:t>b;...) cs` made both engines agree.
//
// The tree-walker engine is no longer user-selectable from the CLI
// (see `src/cli/args.rs:147-160` — `--run-tree` / `--run` were
// dropped as recognised flags in the rerun8 cleanup; the field is
// kept only so internal construction sites compile). The
// surviving engines are the register VM (default) and the Cranelift
// JIT (`--jit`, when built with `--features cranelift`). The bug as
// originally filed cannot recur via `ilo run` because the buggy
// dispatch path is no longer reachable; this test pins the
// surviving engines so a regression on either backend can't slip
// in silently.
//
// The persona's exact shape — a `L t` of cookie / header
// fragments, filtered by a named `non-empty` helper that returns
// `b` — is exercised directly below. A `L n` variant pins the
// numeric predicate path, and an empty-input variant pins the
// short-circuit contract.

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

#[cfg(feature = "cranelift")]
const ENGINES: &[&str] = &["--vm", "--jit"];
#[cfg(not(feature = "cranelift"))]
const ENGINES: &[&str] = &["--vm"];

fn write_src(tag: &str, src: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "ilo_472_flt_named_{tag}_{}_{n}.ilo",
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

fn parity(src: &str, entry: &str, args: &[&str], expected: &str) {
    for engine in ENGINES {
        let actual = run_ok(engine, src, entry, args);
        assert_eq!(
            actual, expected,
            "engine {engine} produced {actual:?}, expected {expected:?} for src `{src}`"
        );
    }
}

// ── Persona shape: `L t` + named bool helper ────────────────────────────

#[test]
fn flt_named_predicate_string_list_persona_shape() {
    // The `hmac-cookie-parser` shape: split a header into parts,
    // drop the empty fragments with a named `non-empty` helper.
    // Pre-fix the tree engine returned the bool from the last
    // call instead of the filtered list.
    let src = "non-empty s:t>b;>len s 0\nmain parts:L t>L t;flt non-empty parts";
    parity(src, "main", &["[\"a\",\"\",\"b\",\"\",\"c\"]"], "[a, b, c]");
}

#[test]
fn flt_named_predicate_string_list_all_pass() {
    let src = "non-empty s:t>b;>len s 0\nmain parts:L t>L t;flt non-empty parts";
    parity(src, "main", &["[\"a\",\"b\",\"c\"]"], "[a, b, c]");
}

#[test]
fn flt_named_predicate_string_list_all_fail() {
    let src = "non-empty s:t>b;>len s 0\nmain parts:L t>L t;flt non-empty parts";
    parity(src, "main", &["[\"\",\"\",\"\"]"], "[]");
}

#[test]
fn flt_named_predicate_string_list_empty_input() {
    let src = "non-empty s:t>b;>len s 0\nmain parts:L t>L t;flt non-empty parts";
    parity(src, "main", &["[]"], "[]");
}

// ── Numeric variant: pins the named-helper path on `L n` ────────────────

#[test]
fn flt_named_predicate_number_list() {
    let src = "is-pos x:n>b;>x 0\nmain xs:L n>L n;flt is-pos xs";
    parity(src, "main", &["[-1,2,-3,4,-5,6]"], "[2, 4, 6]");
}

// ── Inline-lambda counterpart: the persona's workaround ─────────────────
//
// Pinning the inline-lambda shape alongside the named-helper shape
// ensures the two surfaces stay in lockstep across engines; if a
// future refactor diverges them the persona's "swap to lambda"
// workaround would silently start lying again.

#[test]
fn flt_inline_lambda_matches_named_string() {
    let named = "non-empty s:t>b;>len s 0\nmain parts:L t>L t;flt non-empty parts";
    let lambda = "main parts:L t>L t;flt (s:t>b;>len s 0) parts";
    for engine in ENGINES {
        let n = run_ok(engine, named, "main", &["[\"a\",\"\",\"b\"]"]);
        let l = run_ok(engine, lambda, "main", &["[\"a\",\"\",\"b\"]"]);
        assert_eq!(
            n, l,
            "engine {engine}: named-helper `{n}` diverged from inline-lambda `{l}`"
        );
    }
}
