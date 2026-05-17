// Regression tests for the persona diagnostic batch-3 hint set
// (bundled rerun6 friction).
//
// Three diagnostic improvements live in this file:
//
//   1. Inline-lambda body greediness: `fld (a:n k:t>n;+a kc body k) kws 0`.
//      The body `+a kc body k` consumes only `+a kc` as a single prefix
//      call, leaving `body k` as trailing idents that look like a
//      malformed lambda close. Previously surfaced as a bare
//      `expected RParen, got Ident("body")` ILO-P003 and cascaded into
//      P003/T002/T004 cluster. Now points at the `)`-close site with a
//      hint to wrap chained calls in parens. (pdf-analyst rerun6.)
//
//   2. `rsrt fn ctx xs` / `srt fn ctx xs` / `map fn ctx xs` /
//      `flt fn ctx xs` param-order swap. The fn binds `(element, ctx)`
//      but personas write `(ctx, element)` because the call site lists
//      `fn ctx xs` and the first lambda param looks like it should
//      match the ctx slot. We detect when fn's first param type matches
//      the ctx slot's type AND fn's second matches the list element
//      type (and the two are distinct) and surface a swap hint.
//      (content-mod rerun6.)
//
//   3. List-literal `;` separator. `[a;b;c]` is the Python/JS/Rust
//      reflex; ilo list literals use whitespace (commas optional). The
//      generic `expected expression, got Semi` ILO-P009 doesn't name
//      the actual cause. Now points at the `;` with a list-syntax hint.
//      (nlp-engineer rerun6.)
//
// All three are diagnostic-only; no behaviour change to existing
// well-formed programs. Cross-engine coverage via the CLI exercises
// parser + verifier paths shared by tree, VM, and Cranelift.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_inline(engine: &str, src: &str, entry: &str) -> (bool, String) {
    let out = ilo()
        .args([src, engine, entry])
        .output()
        .expect("failed to run ilo");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// ---------------------------------------------------------------------------
// 1. Inline-lambda body greediness — RESOLVED by #332 (parse_prefix_binop now
// expands known-arity calls), so the previously-cascading shape parses cleanly:
//
//   kc x:t>n;len x;body k:t>t;k;main>n;kws=["hi" "there"];fld (a:n k:t>n;+a kc body k) kws 0
//
// returns 7 across all engines. The diagnostic-only hints that this test
// originally checked no longer fire because the parse failure that motivated
// them no longer occurs.  Tests removed; the parser fix is the better outcome.
// ---------------------------------------------------------------------------

// Sanity: well-formed inline lambda with parens still works.
#[test]
fn lambda_with_parens_still_works() {
    let src = "kc x:t>n;len x;body k:t>t;k;main>n;kws=[\"hi\" \"there\"];fld (a:n k:t>n;+a (kc (body k))) kws 0";
    let out = ilo()
        .args([src, "--run-tree", "main"])
        .output()
        .expect("failed");
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.trim().contains("7"), "stdout={stdout}");
}

// ---------------------------------------------------------------------------
// 2. rsrt/srt/map/flt fn-ctx-xs param-order swap detection
// ---------------------------------------------------------------------------

const RSRT_SWAP: &str =
    "mkey c:n e:t>n;+c (len e);main>L t;ctx=10;xs=[\"bb\" \"a\" \"ccc\"];rsrt mkey ctx xs";

fn check_rsrt_swap(engine: &str) {
    let (ok, stderr) = run_inline(engine, RSRT_SWAP, "main");
    assert!(!ok, "engine={engine}: expected verify failure");
    assert!(
        stderr.contains("ILO-T013"),
        "engine={engine}: missing ILO-T013, stderr={stderr}"
    );
    assert!(
        stderr.contains("params look swapped"),
        "engine={engine}: missing swap detection, stderr={stderr}"
    );
    assert!(
        stderr.contains("(element, ctx)"),
        "engine={engine}: missing param-order hint, stderr={stderr}"
    );
}

#[test]
fn rsrt_swap_tree() {
    check_rsrt_swap("--run-tree");
}

#[test]
fn rsrt_swap_vm() {
    check_rsrt_swap("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn rsrt_swap_cranelift() {
    check_rsrt_swap("--run-cranelift");
}

// Same shape but with `srt`.
#[test]
fn srt_swap_tree() {
    let src = "mkey c:n e:t>n;+c (len e);main>L t;ctx=10;xs=[\"bb\" \"a\" \"ccc\"];srt mkey ctx xs";
    let (ok, stderr) = run_inline("--run-tree", src, "main");
    assert!(!ok);
    assert!(stderr.contains("params look swapped"), "stderr={stderr}");
}

// Same shape with `map`.
#[test]
fn map_swap_tree() {
    let src = "f c:n e:t>n;+c (len e);main>L n;ctx=10;xs=[\"bb\" \"a\" \"ccc\"];map f ctx xs";
    let (ok, stderr) = run_inline("--run-tree", src, "main");
    assert!(!ok);
    assert!(stderr.contains("params look swapped"), "stderr={stderr}");
}

// Same shape with `flt`.
#[test]
fn flt_swap_tree() {
    let src = "f c:n e:t>b;>(len e) c;main>L t;ctx=1;xs=[\"bb\" \"a\" \"ccc\"];flt f ctx xs";
    let (ok, stderr) = run_inline("--run-tree", src, "main");
    assert!(!ok);
    assert!(stderr.contains("params look swapped"), "stderr={stderr}");
}

// Sanity: correct param order doesn't fire the swap hint.
#[test]
fn rsrt_correct_order_passes() {
    let src =
        "mkey e:t c:n>n;+c (len e);main>L t;ctx=10;xs=[\"bb\" \"a\" \"ccc\"];rsrt mkey ctx xs";
    let (ok, stderr) = run_inline("--run-tree", src, "main");
    assert!(ok, "stderr={stderr}");
}

// Sanity: when both param types match the ctx and element types (same
// type), we can't tell which is which, so no spurious hint.
#[test]
fn rsrt_same_type_no_false_positive() {
    let src = "mkey a:n b:n>n;+a b;main>L n;ctx=10;xs=[1 2 3];rsrt mkey ctx xs";
    let (ok, stderr) = run_inline("--run-tree", src, "main");
    assert!(ok, "stderr={stderr}");
}

// ---------------------------------------------------------------------------
// 3. List literal `;` separator
// ---------------------------------------------------------------------------

const LIST_SEMI: &str = "main>L n;xs=[1;2;3];sum xs";

fn check_list_semi(engine: &str) {
    let (ok, stderr) = run_inline(engine, LIST_SEMI, "main");
    assert!(!ok, "engine={engine}: expected parse failure");
    assert!(
        stderr.contains("ILO-P009"),
        "engine={engine}: missing ILO-P009, stderr={stderr}"
    );
    assert!(
        stderr.contains("not a list separator"),
        "engine={engine}: missing list-separator message, stderr={stderr}"
    );
    assert!(
        stderr.contains("whitespace"),
        "engine={engine}: missing whitespace hint, stderr={stderr}"
    );
}

#[test]
fn list_semi_tree() {
    check_list_semi("--run-tree");
}

#[test]
fn list_semi_vm() {
    check_list_semi("--run-vm");
}

#[test]
#[cfg(feature = "cranelift")]
fn list_semi_cranelift() {
    check_list_semi("--run-cranelift");
}

// Sanity: whitespace and comma list literals still parse.
#[test]
fn list_whitespace_still_works() {
    let out = ilo()
        .args(["main>n;sum [1 2 3]", "--run-tree", "main"])
        .output()
        .expect("failed");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).trim().contains("6"));
}

#[test]
fn list_comma_still_works() {
    let out = ilo()
        .args(["main>n;sum [1, 2, 3]", "--run-tree", "main"])
        .output()
        .expect("failed");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).trim().contains("6"));
}

// Confirm the diagnostic doesn't fire on a stray `;` outside `[...]`
// (e.g. after a statement) — that's the existing generic behaviour.
#[test]
fn semi_outside_list_unchanged() {
    let src = "main>n;[1 2 3];0";
    let out = ilo()
        .args([src, "--run-tree", "main"])
        .output()
        .expect("failed");
    // This one should succeed — `[1 2 3]` is a discarded value.
    assert!(
        out.status.success()
            || !String::from_utf8_lossy(&out.stderr).contains("not a list separator")
    );
}
