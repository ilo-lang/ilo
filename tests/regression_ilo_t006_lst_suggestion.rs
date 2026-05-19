// Regression: ILO-T006 on `lst` carries a suggestion clarifying that
// `lst xs i v` is "list set at index" (3 args), not "last element". Agents
// frequently misread the name and reach for `lst xs` — the diagnostic now
// points them at the canonical `at xs -1` for "last element" and at the
// 3-arg signature for any other arity mismatch.
//
// git-workflow rerun11 surfaced this: the empty-suggestion ILO-T006 cost
// agents extra retries because there was no name-aware hint.

use std::process::Command;

fn ilo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ilo"))
}

fn run_err_json(src: &str) -> String {
    let out = ilo()
        .args([src, "--json"])
        .output()
        .expect("failed to run ilo");
    assert!(
        !out.status.success(),
        "expected failure for {src:?}, stdout: {}",
        String::from_utf8_lossy(&out.stdout),
    );
    // JSON diagnostics go to stderr; merge both for robust assertions.
    let mut combined = String::from_utf8_lossy(&out.stderr).into_owned();
    combined.push('\n');
    combined.push_str(&String::from_utf8_lossy(&out.stdout));
    combined
}

#[test]
fn lst_one_arg_suggests_at_for_last_element() {
    let out = run_err_json("foo xs:L n>L n;lst xs");
    assert!(out.contains("ILO-T006"), "missing ILO-T006: {out}");
    assert!(
        out.contains("arity mismatch: 'lst' expects 3 args, got 1"),
        "missing arity message: {out}"
    );
    assert!(
        out.contains("for \\\"last element\\\" use `at xs -1`")
            || out.contains("for \"last element\" use `at xs -1`"),
        "missing last-element suggestion: {out}"
    );
    assert!(
        out.contains("`lst xs i v` updates index i"),
        "missing canonical 3-arg form in suggestion: {out}"
    );
}

#[test]
fn lst_two_args_suggests_canonical_form_without_last_misread() {
    let out = run_err_json("foo xs:L n>L n;lst xs 0");
    assert!(out.contains("ILO-T006"), "missing ILO-T006: {out}");
    assert!(
        out.contains("arity mismatch: 'lst' expects 3 args, got 2"),
        "missing arity message: {out}"
    );
    assert!(
        out.contains("`lst xs i v` returns a new list with index i replaced by v"),
        "missing 3-arg signature suggestion: {out}"
    );
    // Critical: do NOT misread `lst xs i` as "last element" intent.
    assert!(
        !out.contains("last element"),
        "2-arg form should not mention last element: {out}"
    );
}

#[test]
fn lst_four_args_suggests_canonical_form_without_last_misread() {
    let out = run_err_json("foo xs:L n>L n;lst xs 0 1 2");
    assert!(out.contains("ILO-T006"), "missing ILO-T006: {out}");
    assert!(
        out.contains("arity mismatch: 'lst' expects 3 args, got 4"),
        "missing arity message: {out}"
    );
    assert!(
        out.contains("`lst xs i v` returns a new list with index i replaced by v"),
        "missing 3-arg signature suggestion: {out}"
    );
    assert!(
        !out.contains("last element"),
        "4-arg form should not mention last element: {out}"
    );
}

#[test]
fn other_builtin_arity_errors_do_not_get_lst_hint() {
    // Sanity: arity errors on a different builtin must not pick up the
    // lst-specific suggestion.
    let out = run_err_json("foo xs:L n>n;hd xs 1 2");
    assert!(out.contains("ILO-T006"), "missing ILO-T006: {out}");
    assert!(
        !out.contains("`lst xs i v`") && !out.contains("last element"),
        "lst hint leaked to other builtin: {out}"
    );
}

#[test]
fn lst_suggestion_is_exposed_as_json_suggestion_field() {
    // Pin the diagnostic JSON shape: the suggestion lives under the
    // top-level "suggestion" key, alongside code/message/severity.
    let out = run_err_json("foo xs:L n>L n;lst xs");
    assert!(
        out.contains("\"suggestion\":\""),
        "suggestion field missing from JSON: {out}"
    );
    assert!(
        out.contains("\"code\":\"ILO-T006\""),
        "expected ILO-T006 code: {out}"
    );
}
