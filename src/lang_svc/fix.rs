//! Mechanical repair loop over `crate::lang_svc::check` diagnostics.
//!
//! Each iteration takes the highest-priority diagnostic that carries a
//! `fix_plan` (the first such diagnostic in `check` order), applies its
//! single edit via the primary label's byte-offset span — never line
//! numbers — and re-verifies. The loop stops at a terminal state (no
//! diagnostic carries a `fix_plan`), at the iteration cap, or when an
//! iteration fails its sanity / post-fix checks.

use crate::diagnostic::{Diagnostic, Severity};
use serde_json::{json, Value};
use std::path::Path;

/// Number of `error`-severity diagnostics in a `check` result.
fn error_count(diagnostics: &[Diagnostic]) -> usize {
    diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count()
}

/// Cap on repair iterations per call.
const MAX_ITERS_CAP: usize = 10;

/// A fix plan with a path may target an imported file, not the source text
/// supplied to this tool. Only apply it when that target is provably the
/// same file; otherwise a coincidental matching span could corrupt the root.
fn fix_targets_source(plan_path: Option<&str>, source_path: Option<&str>) -> bool {
    let Some(plan_path) = plan_path else {
        return true;
    };
    let Some(source_path) = source_path else {
        return false;
    };
    if Path::new(plan_path) == Path::new(source_path) {
        return true;
    }
    false
}

/// Apply a line-scoped fix edit: replace the full text of the 1-based lines
/// `[line_start, line_end]` with `edit.after`.
///
/// Used when a fix necessarily restructures more than the token the
/// diagnostic anchored on (e.g. rewriting a `let X = E in R` statement into
/// the binding chain `X=E;R`), so the edit's `before` text is the whole
/// line rather than the primary span.
///
/// Returns `None` when the lines do not contain the recorded `before` text,
/// which means the source drifted since the plan was computed.
fn apply_line_scoped_edit(current: &str, edit: &crate::diagnostic::FixEdit) -> Option<String> {
    if edit.line_start == 0 {
        return None;
    }
    let lines: Vec<&str> = current.split('\n').collect();
    let lo = edit.line_start - 1;
    let hi = edit.line_end.min(lines.len());
    if lo >= lines.len() || hi <= lo {
        return None;
    }
    let region = lines[lo..hi].join("\n");
    if region != edit.before {
        return None;
    }
    let mut out: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    out.splice(lo..hi, [edit.after.clone()].into_iter());
    Some(out.join("\n"))
}

/// Run the repair loop.
///
/// * `source` — complete ilo source text.
/// * `path` — optional filesystem path, forwarded to `lang_svc::check` for
///   imports and `fix_plan.path`.
/// * `max_iters` — requested iterations, clamped to [`MAX_ITERS_CAP`].
///   `0` is a dry run: nothing is applied and `fixed_source` is null.
///
/// Returns a structured JSON value. Dry run shape:
/// `{"applied":0,"remaining":N,"preview":[...],"fixed_source":null}`.
/// Normal shape adds `remaining_fixable`, `ok`, and `diagnostics` (the final
/// `check` diagnostics in `ilo_validate` shape). `ok` means no static errors
/// remain; neither it nor the error-count gate proves semantic correctness.
pub fn repair(source: &str, path: Option<&str>, max_iters: usize) -> Value {
    let max_iters = max_iters.min(MAX_ITERS_CAP);

    if max_iters == 0 {
        let initial = crate::lang_svc::check(source, path);
        let preview = fixable_preview(&initial, path);
        return json!({
            "applied": 0,
            "remaining": initial.len(),
            "preview": preview,
            "fixed_source": Value::Null,
        });
    }

    let mut current = source.to_string();
    let mut applied: usize = 0;
    let mut history: Vec<Value> = Vec::new();

    for iteration in 1..=max_iters {
        let diagnostics = crate::lang_svc::check(&current, path);
        let errors_before = error_count(&diagnostics);
        let Some(target) = diagnostics.iter().find(|diagnostic| {
            diagnostic
                .fix_plan
                .as_ref()
                .is_some_and(|plan| fix_targets_source(plan.path.as_deref(), path))
        }) else {
            break; // no fix plan safely applicable to this source remains
        };
        let plan = target.fix_plan.as_ref().expect("fix_plan present");
        let Some(edit) = plan.edits.first() else {
            break; // empty edit list: nothing to apply
        };
        let Some(span) = target
            .labels
            .iter()
            .find(|l| l.is_primary)
            .map(|l| l.span)
        else {
            history.push(json!({
                "iteration": iteration,
                "code": target.code,
                "message": target.message,
                "ok": false,
                "error": "fixable diagnostic has no primary span",
            }));
            break;
        };

        // Sanity: the primary span must still resolve to the recorded
        // `before` text in the current source. Slice with `.get` rather
        // than indexing: byte spans should be char-boundary aligned, but a
        // panic here would take down the whole MCP server on odd input.
        let (head, mid, tail) = match (
            current.get(..span.start),
            current.get(span.start..span.end),
            current.get(span.end..),
        ) {
            (Some(head), Some(mid), Some(tail)) => (head, mid, tail),
            _ => {
                history.push(json!({
                    "iteration": iteration,
                    "code": target.code,
                    "message": target.message,
                    "ok": false,
                    "error": "primary span out of bounds for current source",
                }));
                break;
            }
        };
        // The edit may be span-scoped (`before` == primary span text) or
        // line-scoped (`before` == the full text of lines
        // [line_start, line_end]). Most fixes are span-scoped, but a fix that
        // restructures a whole statement (e.g. rewriting a `let ... in ...`
        // into a binding chain) necessarily covers more than the token the
        // diagnostic anchored on. Prefer the span when the two agree; fall
        // back to the line-scoped edit when the span does not match but the
        // line text does.
        let next = if mid == edit.before {
            let mut next = String::with_capacity(
                current.len().saturating_sub(mid.len()) + edit.after.len(),
            );
            next.push_str(head);
            next.push_str(&edit.after);
            next.push_str(tail);
            next
        } else {
            match apply_line_scoped_edit(&current, edit) {
                Some(next) => next,
                None => {
                    history.push(json!({
                        "iteration": iteration,
                        "code": target.code,
                        "message": target.message,
                        "ok": false,
                        "error": "primary span no longer matches recorded before text",
                    }));
                    break;
                }
            }
        };

        if next == current {
            // No edit was applied: fail the iteration.
            history.push(json!({
                "iteration": iteration,
                "code": target.code,
                "message": target.message,
                "ok": false,
                "error": "edit did not change the source",
            }));
            break;
        }

        // Conservative error-count gate, not a semantic correctness check:
        // reject a candidate with more static errors, including errors that
        // a successful lexical/parse repair merely reveals. Equal or lower
        // counts can still hide new errors or changed behavior; callers need
        // execution tests against expected results for correctness claims.
        let post_diagnostics = crate::lang_svc::check(&next, path);
        if error_count(&post_diagnostics) > errors_before {
            history.push(json!({
                "iteration": iteration,
                "code": target.code,
                "message": target.message,
                "ok": false,
                "error": "fix regressed error count; reverted",
            }));
            break;
        }
        history.push(json!({
            "iteration": iteration,
            "code": target.code,
            "message": target.message,
            "start": span.start,
            "end": span.end,
            "line_start": edit.line_start,
            "line_end": edit.line_end,
            "before": edit.before,
            "after": edit.after,
            "ok": true,
        }));
        current = next;
        applied += 1;
        // Re-verification happens at the top of the next iteration via a
        // fresh `check` call; the loop then continues while diagnostics with
        // `fix_plan` remain and stops when only unfixable diagnostics (or
        // none) remain.
    }

    let final_diagnostics = crate::lang_svc::check(&current, path);
    let remaining_fixable = final_diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic
                .fix_plan
                .as_ref()
                .is_some_and(|plan| fix_targets_source(plan.path.as_deref(), path))
        })
        .count();
    let ok = !final_diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error);

    json!({
        "applied": applied,
        "remaining": final_diagnostics.len(),
        "remaining_fixable": remaining_fixable,
        "ok": ok,
        "preview": history,
        "fixed_source": if applied == 0 { Value::Null } else { Value::String(current) },
        "diagnostics": final_diagnostics.iter().map(diagnostic_json).collect::<Vec<_>>(),
    })
}

/// Repair using only narrowly type-proven edits that preserve operand roles.
/// Unlike `repair`, this deliberately skips generic typo/reserved-name fixes.
pub fn repair_safe(source: &str, path: Option<&str>, max_iters: usize) -> Value {
    const SAFE_ITER_CAP: usize = 100;
    let max_iters = max_iters.min(SAFE_ITER_CAP);
    if max_iters == 0 {
        let diagnostics = crate::lang_svc::check(source, path);
        let preview: Vec<Value> = diagnostics
            .iter()
            .filter(|diagnostic| is_safe_fix(diagnostic))
            .filter_map(|diagnostic| {
                let plan = diagnostic.fix_plan.as_ref()?;
                let edit = plan.edits.first()?;
                Some(json!({
                    "code": diagnostic.code,
                    "message": diagnostic.message,
                    "before": edit.before,
                    "after": edit.after,
                    "line_start": edit.line_start,
                    "line_end": edit.line_end,
                }))
            })
            .collect();
        let ok = !diagnostics.iter().any(|d| d.severity == Severity::Error);
        return json!({
            "applied": 0,
            "remaining": diagnostics.len(),
            "remaining_fixable": preview.len(),
            "ok": ok,
            "preview": preview,
            "fixed_source": Value::Null,
            "diagnostics": diagnostics.iter().map(diagnostic_json).collect::<Vec<_>>(),
        });
    }
    let mut current = source.to_string();
    let mut history = Vec::new();

    for _ in 0..max_iters {
        let diagnostics = crate::lang_svc::check(&current, path);
        let fixable: Vec<&Diagnostic> = diagnostics
            .iter()
            .filter(|d| {
                d.fix_plan
                    .as_ref()
                    .is_some_and(|plan| fix_targets_source(plan.path.as_deref(), path))
            })
            .collect();
        let Some(index) = fixable.iter().position(|diagnostic| is_safe_fix(diagnostic)) else {
            break;
        };
        let applied = apply_one(&current, path, index);
        let Some(next) = applied.get("fixed_source").and_then(Value::as_str) else {
            break;
        };
        if next == current {
            break;
        }
        history.push(json!({
            "code": applied.get("code"),
            "before": applied.get("before"),
            "after": applied.get("after"),
            "start": applied.get("start"),
            "end": applied.get("end"),
            "line_start": applied.get("line_start"),
            "line_end": applied.get("line_end"),
            "ok": true,
        }));
        current = next.to_string();
    }

    let diagnostics = crate::lang_svc::check(&current, path);
    let remaining_fixable = diagnostics
        .iter()
        .filter(|d| {
            d.fix_plan
                .as_ref()
                .is_some_and(|plan| fix_targets_source(plan.path.as_deref(), path))
        })
        .count();
    let ok = !diagnostics.iter().any(|d| d.severity == Severity::Error);
    json!({
        "applied": history.len(),
        "remaining": diagnostics.len(),
        "remaining_fixable": remaining_fixable,
        "ok": ok,
        "preview": history,
        "fixed_source": if current == source { Value::Null } else { Value::String(current) },
        "diagnostics": diagnostics.iter().map(diagnostic_json).collect::<Vec<_>>(),
    })
}

fn is_safe_fix(diagnostic: &Diagnostic) -> bool {
    if diagnostic.fix_plan.is_none() {
        return false;
    }
    matches!(
        (diagnostic.code, diagnostic.suggestion.as_deref()),
        (Some("ILO-T005"), Some(crate::diagnostic::FIX_HINT_JOIN_TO_CAT))
            | (Some("ILO-T005"), Some(crate::diagnostic::FIX_HINT_NOT_TO_BANG))
            | (Some("ILO-T013"), Some(crate::diagnostic::FIX_HINT_SWAP_AT_ARGS))
            | (Some("ILO-T006"), Some(crate::diagnostic::FIX_HINT_RANGE_ZERO_START))
            | (Some("ILO-T008"), Some(crate::diagnostic::FIX_HINT_TERNARY_BOOL_LITERALS))
            | (Some("ILO-P011"), Some(_))
    )
}

/// Apply a single fix_plan edit by index.
///
/// * `index` — zero-based index into the list of currently fixable diagnostics
///   (in `check` order). `0` applies the first fixable error, which is also
///   what the iterative `repair` loop would do. Returns `fixed_source: null`
///   when no fix was applied.
///
/// The single-step tool is the lowest-latency path for the agent when
/// `ilo_validate` reports exactly one `fix_plan`. Agents can also call
/// `repair` (`ilo_fix`) for a bounded repair loop when multiple diagnostics
/// carry fix plans.
pub fn apply_one(source: &str, path: Option<&str>, index: usize) -> Value {
    let diagnostics = crate::lang_svc::check(source, path);
    let fixable: Vec<&crate::diagnostic::Diagnostic> = diagnostics
        .iter()
        .filter(|d| {
            d.fix_plan
                .as_ref()
                .is_some_and(|plan| fix_targets_source(plan.path.as_deref(), path))
        })
        .collect();

    if fixable.is_empty() {
        return json!({
            "applied": 0,
            "index": index,
            "fixable": 0,
            "fixed_source": Value::Null,
            "ok": !diagnostics.iter().any(|d| d.severity == Severity::Error),
            "diagnostics": diagnostics.iter().map(diagnostic_json).collect::<Vec<_>>(),
            "detail": "no fixable diagnostics"
        });
    }

    if index >= fixable.len() {
        return json!({
            "applied": 0,
            "index": index,
            "fixable": fixable.len(),
            "fixed_source": Value::Null,
            "ok": false,
            "error": format!("index {index} out of range (0..{})", fixable.len()),
            "diagnostics": diagnostics.iter().map(diagnostic_json).collect::<Vec<_>>(),
        });
    }

    let target = fixable[index];
    let plan = target.fix_plan.as_ref().expect("fix_plan present");
    let edit = match plan.edits.first() {
        Some(e) => e,
        None => {
            return json!({
                "applied": 0,
                "index": index,
                "fixable": fixable.len(),
                "fixed_source": Value::Null,
                "ok": false,
                "error": "fixable diagnostic has empty edits",
                "diagnostics": diagnostics.iter().map(diagnostic_json).collect::<Vec<_>>(),
            })
        }
    };
    let span = match target.labels.iter().find(|l| l.is_primary).map(|l| l.span) {
        Some(s) => s,
        None => {
            return json!({
                "applied": 0,
                "index": index,
                "fixed_source": Value::Null,
                "ok": false,
                "error": "fixable diagnostic has no primary span",
                "diagnostics": diagnostics.iter().map(diagnostic_json).collect::<Vec<_>>(),
            })
        }
    };

    let (head, mid, tail) = match (
        source.get(..span.start),
        source.get(span.start..span.end),
        source.get(span.end..),
    ) {
        (Some(h), Some(m), Some(t)) => (h, m, t),
        _ => {
            return json!({
                "applied": 0,
                "index": index,
                "fixed_source": Value::Null,
                "ok": false,
                "error": "primary span out of bounds for source",
                "diagnostics": diagnostics.iter().map(diagnostic_json).collect::<Vec<_>>(),
            })
        }
    };
    if mid != edit.before {
        return json!({
            "applied": 0,
            "index": index,
            "fixed_source": Value::Null,
            "ok": false,
            "error": "primary span no longer matches recorded before text",
            "before_expected": edit.before,
            "before_actual": mid,
            "diagnostics": diagnostics.iter().map(diagnostic_json).collect::<Vec<_>>(),
        });
    }

    let mut next = String::with_capacity(source.len().saturating_sub(mid.len()) + edit.after.len());
    next.push_str(head);
    next.push_str(&edit.after);
    next.push_str(tail);

    if next == source {
        return json!({
            "applied": 0,
            "index": index,
            "fixed_source": Value::Null,
            "ok": false,
            "error": "edit did not change source",
            "diagnostics": diagnostics.iter().map(diagnostic_json).collect::<Vec<_>>(),
        });
    }

    let errors_before = error_count(&diagnostics);
    let post = crate::lang_svc::check(&next, path);
    if error_count(&post) > errors_before {
        return json!({
            "applied": 0,
            "index": index,
            "fixed_source": Value::Null,
            "ok": false,
            "error": "fix regressed error count; reverted",
            "before": edit.before,
            "after": edit.after,
            "code": target.code,
            "diagnostics": post.iter().map(diagnostic_json).collect::<Vec<_>>(),
        });
    }

    let ok = !post.iter().any(|d| d.severity == Severity::Error);
    json!({
        "applied": 1,
        "index": index,
        "fixable": fixable.len(),
        "code": target.code,
        "message": target.message,
        "before": edit.before,
        "after": edit.after,
        "start": span.start,
        "end": span.end,
        "line_start": edit.line_start,
        "line_end": edit.line_end,
        "fixed_source": next,
        "ok": ok,
        "remaining": post.len(),
        "remaining_fixable": post.iter().filter(|d| d.fix_plan.as_ref().is_some_and(|p| fix_targets_source(p.path.as_deref(), path))).count(),
        "diagnostics": post.iter().map(diagnostic_json).collect::<Vec<_>>(),
    })
}

/// Preview entries for every currently fixable diagnostic, in `check` order.
fn fixable_preview(diagnostics: &[Diagnostic], source_path: Option<&str>) -> Vec<Value> {
    diagnostics
        .iter()
        .filter(|d| d.fix_plan.is_some())
        .map(|d| {
            let plan = d.fix_plan.as_ref().expect("fix_plan present");
            let edit = plan.edits.first();
            let span = d.labels.iter().find(|l| l.is_primary).map(|l| l.span);
            json!({
                "code": d.code,
                "message": d.message,
                "path": plan.path,
                "applicable": fix_targets_source(plan.path.as_deref(), source_path),
                "start": span.map(|s| s.start),
                "end": span.map(|s| s.end),
                "line_start": edit.map(|e| e.line_start),
                "line_end": edit.map(|e| e.line_end),
                "before": edit.map(|e| e.before.clone()),
                "after": edit.map(|e| e.after.clone()),
            })
        })
        .collect()
}

pub fn diagnostic_json(diagnostic: &Diagnostic) -> Value {
    json!({
        "severity": match diagnostic.severity { Severity::Error => "error", Severity::Warning => "warning" },
        "code": diagnostic.code,
        "message": diagnostic.message,
        "labels": diagnostic.labels.iter().map(|label| json!({
            "start": label.span.start,
            "end": label.span.end,
            "message": label.message,
            "primary": label.is_primary
        })).collect::<Vec<_>>(),
        "notes": diagnostic.notes,
        "suggestion": diagnostic.suggestion,
        "fix": diagnostic.fix_plan.as_ref().map(|plan| json!({
            "path": plan.path,
            "edits": plan.edits.iter().map(|edit| json!({
                "line_start": edit.line_start,
                "line_end": edit.line_end,
                "before": edit.before,
                "after": edit.after
            })).collect::<Vec<_>>()
        }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imported_fix_plan_is_not_applicable_to_inline_source() {
        assert!(!fix_targets_source(Some("/tmp/imported.ilo"), None));
        assert!(!fix_targets_source(
            Some("/tmp/imported.ilo"),
            Some("/tmp/root.ilo")
        ));
        assert!(fix_targets_source(Some("/tmp/root.ilo"), Some("/tmp/root.ilo")));
        assert!(fix_targets_source(None, None));
    }

    #[test]
    fn repairs_typo_to_clean() {
        // ILO-T004 carries a fix plan renaming `xyzz` to `x`.
        let out = repair("f x:n>n;xyzz", None, 10);
        assert_eq!(out["applied"].as_u64(), Some(1));
        assert_eq!(out["fixed_source"].as_str(), Some("f x:n>n;x"));
        assert_eq!(out["ok"].as_bool(), Some(true));
        assert_eq!(out["remaining"].as_u64(), Some(0));
    }

    #[test]
    fn repairs_utf8_source_without_corrupting_boundaries() {
        let source = "-- λ\nf x:n>n;xyzz";
        let out = repair(source, None, 10);
        assert_eq!(out["applied"].as_u64(), Some(1));
        assert_eq!(out["fixed_source"].as_str(), Some("-- λ\nf x:n>n;x"));
        assert_eq!(out["ok"].as_bool(), Some(true));
    }

    #[test]
    fn dry_run_lists_without_applying() {
        let out = repair("f x:n>n;xyzz", None, 0);
        assert_eq!(out["applied"].as_u64(), Some(0));
        assert!(out["fixed_source"].is_null());
        assert_eq!(out["remaining"].as_u64(), Some(1));
        assert_eq!(out["preview"][0]["after"].as_str(), Some("x"));
    }

    #[test]
    fn clean_source_is_noop() {
        let out = repair("main>n;prnt 5", None, 10);
        assert_eq!(out["applied"].as_u64(), Some(0));
        assert_eq!(out["ok"].as_bool(), Some(true));
        assert!(out["fixed_source"].is_null());
    }

    #[test]
    fn repairs_underscore_binding_and_reference() {
        // Lexing stops at the first underscore identifier, including when
        // its trailing fragment is a keyword. Repair must re-lex to find
        // the reference; neither occurrence is a reserved-word parse error.
        let source = "my_var=5;my_var";
        let initial = crate::lang_svc::check(source, None);
        assert_eq!(initial.len(), 1, "{initial:#?}");
        assert_eq!(initial[0].code, Some("ILO-L002"));
        let out = repair(source, None, 10);
        assert_eq!(out["applied"], 2, "{out}");
        assert_eq!(out["fixed_source"], "my-var=5;my-var");
        assert_eq!(out["preview"][0]["code"], "ILO-L002");
        assert_eq!(out["preview"][1]["code"], "ILO-L002");
        assert_eq!(out["ok"], true);
        assert_eq!(out["diagnostics"], json!([]));
    }

    #[test]
    fn equal_error_count_does_not_mean_clean() {
        // A lexical repair can reveal a different static error while the
        // error count stays unchanged. Accepted edits must not imply ok.
        let out = repair("my_var=5;my_var;+1 true", None, 10);
        assert_eq!(out["applied"], 2, "{out}");
        assert_eq!(out["fixed_source"], "my-var=5;my-var;+1 true");
        assert_eq!(out["ok"], false);
        assert_eq!(out["remaining"], 1);
        assert_eq!(out["diagnostics"][0]["code"], "ILO-T009");
    }

    #[test]
    fn regressing_fix_is_reverted() {
        // A valid lexical repair can expose multiple independent type errors.
        // Exercise rollback without depending on an invalid return-cast plan.
        let source = "my_var=5;+true 1;+false 2";
        let candidate = "my-var=5;+true 1;+false 2";
        let before = crate::lang_svc::check(source, None);
        assert_eq!(error_count(&before), 1, "{before:#?}");
        assert_eq!(before[0].code, Some("ILO-L002"));
        let after = crate::lang_svc::check(candidate, None);
        assert_eq!(error_count(&after), 2, "{after:#?}");
        assert!(after.iter().all(|d| d.code == Some("ILO-T009")));
        let out = repair(source, None, 10);
        assert_eq!(out["applied"], 0, "{out}");
        assert!(out["fixed_source"].is_null());
        assert_eq!(out["ok"], false);
        assert_eq!(out["diagnostics"][0]["code"], "ILO-L002");
        let preview = out["preview"].as_array().unwrap();
        assert_eq!(preview.len(), 1);
        assert_eq!(preview[0]["code"], "ILO-L002");
        assert_eq!(preview[0]["ok"], false);
        assert_eq!(preview[0]["error"], "fix regressed error count; reverted");
        let single = apply_one(source, None, 0);
        assert_eq!(single["applied"], 0, "{single}");
        assert!(single["fixed_source"].is_null());
        assert_eq!(single["error"], "fix regressed error count; reverted");
    }

    #[test]
    fn reserved_binding_repair_stops_without_inventing_a_return_value() {
        let out = repair("f>n;var=5", None, 10);
        assert_eq!(out["applied"], 1, "{out}");
        assert_eq!(out["fixed_source"], "f>n;v=5");
        assert_eq!(out["ok"], false);
        assert_eq!(out["remaining_fixable"], 0);
        assert_eq!(out["preview"].as_array().unwrap().len(), 1);
        assert_eq!(out["preview"][0]["code"], "ILO-P011");
        assert_eq!(out["diagnostics"].as_array().unwrap().len(), 1);
        assert_eq!(out["diagnostics"][0]["code"], "ILO-T008");
        assert!(out["diagnostics"][0]["fix"].is_null());
        let single = apply_one("f>n;v=5", None, 0);
        assert_eq!(single["applied"], 0);
        assert_eq!(single["fixable"], 0);
        assert!(single["fixed_source"].is_null());
    }

    #[test]
    fn number_to_text_expression_tails_repair_cleanly() {
        for (source, expected) in [
            ("f x:n>t;x", "f x:n>t;str x"),
            ("f x:n>t;*x 2", "f x:n>t;str *x 2"),
            ("f>t;v=5;v", "f>t;v=5;str v"),
        ] {
            for out in [repair(source, None, 10), apply_one(source, None, 0)] {
                assert_eq!(out["applied"], 1, "{source}: {out}");
                assert_eq!(out["fixed_source"], expected);
                assert_eq!(out["ok"], true);
                assert_eq!(out["diagnostics"], json!([]));
            }
        }
    }

    #[test]
    fn unsafe_return_conversions_have_no_plan() {
        for source in ["f>n;v=5", "f>t;v=5", "f x:t>n;x", "f x:b>n;x"] {
            let diagnostics = crate::lang_svc::check(source, None);
            assert_eq!(diagnostics.len(), 1, "{source}: {diagnostics:#?}");
            assert_eq!(diagnostics[0].code, Some("ILO-T008"));
            assert!(diagnostics[0].fix_plan.is_none(), "{source}");
            for out in [repair(source, None, 10), apply_one(source, None, 0)] {
                assert_eq!(out["applied"], 0, "{source}: {out}");
                assert!(out["fixed_source"].is_null());
                assert_eq!(out["ok"], false);
            }
        }
    }

    #[test]
    fn unfixable_diagnostic_terminates() {
        // Arity errors carry no fix plan: loop must stop at once.
        let out = repair("f x:n>n;+x 1 2", None, 10);
        assert_eq!(out["applied"].as_u64(), Some(0));
        assert_eq!(out["remaining_fixable"].as_u64(), Some(0));
        assert!(out["fixed_source"].is_null());
    }
}
