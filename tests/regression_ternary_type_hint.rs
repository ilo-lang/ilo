// Regression: ILO-T003 ternary branch type mismatch carries an
// actionable hint that names the cheapest fix.
//
// Before this change, a ternary like `?h c 1 "x"` produced
// ILO-T003 with the generic hint "both branches of a ternary must
// return the same type". Agents had no signal on which direction
// to convert, so they either guessed (often wrongly) or
// restructured unnecessarily. With this fix the verifier emits a
// targeted hint:
//
//   - number vs text → surface both `str <num-branch>` and
//     `num <text-branch>!` so the agent picks the direction
//     matching intent (and is reminded `num` returns `R n t`
//     that needs `!` to unwrap);
//   - matching branches → no error (false-positive guard);
//   - any other mismatch (bool vs text, list vs map, two named
//     records, `R T E` vs `n`, …) → fall back to a restructure
//     hint, because `str`/`num` would just trip ILO-T013 and
//     mislead the agent.
//
// The verifier is shared across every engine, so a single
// invocation per case is enough — there's no codegen surface to
// vary across tree/VM/Cranelift.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn verify_stderr(src: &str) -> String {
    let mut cmd = ilo();
    cmd.arg(src).arg("--vm");
    let out = cmd.output().expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected verify to fail on {src:?}, but it succeeded: stdout={}",
        String::from_utf8_lossy(&out.stdout)
    );
    // Diagnostics are emitted as JSON on stderr; collapse to a single
    // searchable string so assertions don't depend on exact framing.
    let mut combined = String::new();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    combined.push_str(&String::from_utf8_lossy(&out.stdout));
    combined
}

fn verify_ok(src: &str) {
    // Write to a temp file and `ilo check` it — `ilo check` requires
    // a file path, and routing the inline form through `ilo run` would
    // execute the body (extra surface for unrelated runtime errors).
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("prog.ilo");
    std::fs::write(&path, src).expect("write temp ilo");
    let mut cmd = ilo();
    cmd.arg("check").arg(&path);
    let out = cmd.output().expect("failed to run ilo check");
    assert!(
        out.status.success(),
        "expected `ilo check` to pass on {src:?}: stderr={} stdout={}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
}

// ── number-vs-text in either branch position surfaces both
// directions, so the agent picks the intended one. ─────────────

#[test]
fn ternary_num_then_text_else_hints_both_directions() {
    // `?h c 1 "x"` — then=n, else=t.
    let src = "f c:b>t;?h c 1 \"x\"";
    let stderr = verify_stderr(src);
    assert!(
        stderr.contains("ILO-T003"),
        "expected ILO-T003, got: {stderr}"
    );
    assert!(
        stderr.contains("str <num-branch>")
            && stderr.contains("default-on-err (num <text-branch>)"),
        "expected both `str <num-branch>` and `num <text-branch>!` in hint, got: {stderr}"
    );
}

#[test]
fn ternary_text_then_num_else_hints_both_directions() {
    // `?h c "x" 1` — then=t, else=n. Same hint shape (symmetric).
    let src = "f c:b>t;?h c \"x\" 1";
    let stderr = verify_stderr(src);
    assert!(
        stderr.contains("ILO-T003"),
        "expected ILO-T003, got: {stderr}"
    );
    assert!(
        stderr.contains("str <num-branch>")
            && stderr.contains("default-on-err (num <text-branch>)"),
        "expected both directions in hint, got: {stderr}"
    );
}

// ── bool-vs-text → no scalar conversion exists (bool has no
// `str`/`num` coercion), so we fall back to the restructure hint
// rather than suggest a conversion that would trip ILO-T013. ──

#[test]
fn ternary_bool_vs_text_falls_back_to_restructure_hint() {
    // then=b (param `c`), else=t. There's no scalar bool→text
    // coercion in ilo, so the safe advice is to restructure.
    let src = "f c:b>t;?h c c \"no\"";
    let stderr = verify_stderr(src);
    assert!(
        stderr.contains("ILO-T003"),
        "expected ILO-T003, got: {stderr}"
    );
    assert!(
        stderr.contains("no scalar coercion") && stderr.contains("restructure"),
        "expected restructure fallback hint, got: {stderr}"
    );
    // Must not offer a `str`/`num` conversion that wouldn't apply.
    assert!(
        !stderr.contains("str <num-branch>"),
        "should not offer str-conversion for bool/text case: {stderr}"
    );
    assert!(
        !stderr.contains("default-on-err"),
        "should not offer num-conversion for bool/text case: {stderr}"
    );
}

// ── Matching branches still typecheck cleanly — no false-positive
// hint emission. ───────────────────────────────────────────────

#[test]
fn ternary_matching_text_branches_ok() {
    verify_ok("f c:b>t;?h c \"a\" \"b\"");
}

#[test]
fn ternary_matching_number_branches_ok() {
    verify_ok("f c:b>n;?h c 1 2");
}

// ── No scalar coercion bridges the types → fall back to the
// restructure hint (list/record/`O T`/`R T E`). ────────────────

#[test]
fn ternary_list_vs_map_falls_back_to_restructure_hint() {
    // then = `L n`, else = `M t n` — no `str`/`num` would help, so
    // the hint should explicitly point at restructuring rather
    // than offer a misleading conversion.
    // `L n` (list literal) vs `M _ n` (built via mmap+mset, ilo's
    // canonical map construction — there is no map literal syntax).
    let src = "f c:b>L n;l=[1, 2];m=mset mmap \"a\" 1;?h c l m";
    let stderr = verify_stderr(src);
    assert!(
        stderr.contains("ILO-T003"),
        "expected ILO-T003, got: {stderr}"
    );
    assert!(
        stderr.contains("restructure") && stderr.contains("no scalar coercion"),
        "expected restructure fallback hint, got: {stderr}"
    );
    // Must not offer a `str`/`num` conversion that wouldn't apply.
    assert!(
        !stderr.contains("str <num-branch>"),
        "fallback hint should not suggest str: {stderr}"
    );
    assert!(
        !stderr.contains("default-on-err"),
        "fallback hint should not suggest num: {stderr}"
    );
}
