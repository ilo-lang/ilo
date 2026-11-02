//! Protocol-neutral language services, shared by the CLI and MCP adapter.

mod builtins;
mod fix;
pub mod lsp_features;
mod semantic;
#[doc(hidden)]
pub mod imports;

pub use fix::{apply_one, diagnostic_json, repair, repair_safe};
pub use semantic::inspect;

use crate::{ast, diagnostic::Diagnostic, lexer, parser, verify};

fn enrich(d: Diagnostic, source: &str, path: Option<&str>) -> Diagnostic {
    let mut d = d.with_source(source);
    if let Some(path) = path { d = d.with_path(path); }
    d.derive_fix_plan()
}

fn prepare(source: &str, path: Option<&str>) -> Result<(ast::Program, Vec<Diagnostic>), Diagnostic> {
    let tokens = lexer::lex(source).map_err(|e| enrich(Diagnostic::from(&e), source, path))?;
    let tokens = tokens.into_iter().map(|(token, range)| (token, ast::Span { start: range.start, end: range.end })).collect();
    let (mut program, errors) = parser::parse(tokens);
    ast::resolve_aliases(&mut program);
    ast::desugar_dot_var_index(&mut program);
    program.source = Some(source.to_string());
    let canonical = path.and_then(|p| std::path::Path::new(p).canonicalize().ok());
    let base_dir = canonical.as_ref().and_then(|p| p.parent());
    let mut visited = std::collections::HashSet::new();
    if let Some(canonical) = &canonical { visited.insert(canonical.clone()); }
    let mut diagnostics = Vec::new();
    program.declarations = imports::resolve_imports(program.declarations, base_dir, &mut visited, &mut diagnostics, imports::BuildTarget::default());
    diagnostics.extend(errors.iter().map(|e| enrich(Diagnostic::from(e), source, path)));
    Ok((program, diagnostics))
}

/// Check without execution. Like the CLI, verification continues after parse errors.
pub fn check(source: &str, path: Option<&str>) -> Vec<Diagnostic> {
    let (program, mut diagnostics) = match prepare(source, path) {
        Ok(value) => value,
        Err(d) => return vec![d],
    };
    let result = verify::verify_with_effects(&program, false);
    diagnostics.extend(result.warnings.iter().chain(result.errors.iter()).map(|e| enrich(Diagnostic::from(e), source, path)));
    diagnostics
}

/// Infer effects with the same import context as validation.
pub fn inferred_effects(source: &str, path: Option<&str>) -> Vec<verify::FnEffects> {
    match prepare(source, path) {
        Ok((program, _)) => verify::verify_with_effects(&program, true).effects,
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Severity;

    #[test]
    fn inline_fix_has_no_file_path() {
        let diagnostics = check("f x:n>n;xyzz", None);
        let plan = diagnostics[0].fix_plan.as_ref().unwrap();
        assert!(plan.path.is_none());
    }

    #[test]
    fn verification_continues_after_parse_errors() {
        // Keep the verifier error in a valid declaration: recovery may
        // discard the malformed function, but must still verify `f`.
        // An underscore identifier would fail lexing before either phase.
        let source = "f>n;xyzz\ng>n;var=5";
        let diagnostics = check(source, None);
        for (code, token) in [("ILO-P011", "var"), ("ILO-T004", "xyzz")] {
            let diagnostic = diagnostics.iter().find(|d| d.code == Some(code))
                .unwrap_or_else(|| panic!("missing {code}: {diagnostics:#?}"));
            let span = diagnostic.labels.iter().find(|l| l.is_primary).unwrap().span;
            assert_eq!(source.get(span.start..span.end), Some(token));
        }
    }

    #[test]
    fn fix_plan_renames_join_only_for_text_list_and_separator() {
        let source = r#"f xs:L t>t;join xs " ""#;
        let diagnostics = check(source, None);
        let diagnostic = diagnostics.iter().find(|d| d.code == Some("ILO-T005")).unwrap();
        let plan = diagnostic.fix_plan.as_ref().expect("type-proven join quick fix");
        assert_eq!(plan.edits[0].before, r#"join xs " ""#);
        assert_eq!(plan.edits[0].after, r#"cat xs " ""#);
        let fixed = repair(source, None, 10);
        assert_eq!(fixed["ok"], true, "{fixed}");
        assert_eq!(fixed["fixed_source"], r#"f xs:L t>t;cat xs " ""#);
    }


    #[test]
    fn fix_plan_rewrites_single_arg_range() {
        let source = "f xs:L n>L n;map {i> / (at i xs) (at i xs)} (range (len xs))";
        let diagnostics = check(source, None);
        let diagnostic = diagnostics
            .iter()
            .find(|d| d.code == Some("ILO-T006")
                && d.suggestion.as_deref() == Some(crate::diagnostic::FIX_HINT_RANGE_ZERO_START))
            .unwrap();
        let plan = diagnostic.fix_plan.as_ref().expect("single-arg range fix");
        assert!(plan.edits[0].after.contains("range 0 (len xs)"));
        let fixed = repair_safe(source, None, 5);
        assert_eq!(fixed["ok"], true, "{fixed}");
        assert!(fixed["fixed_source"].as_str().unwrap().contains("range 0 (len xs)"));
    }

    #[test]
    fn fix_p011_rename_basic() {
        // P011 fires when a reserved builtin name is used as a binding.
        // `unique` is an alias for the `unq` builtin, so binding it is a
        // reserved-name error, not an undefined-variable error.
        let source = "unique = 5;";
        let diagnostics = check(source, None);
        let diagnostic = diagnostics.iter().find(|d| d.code == Some("ILO-P011"));
        assert!(diagnostic.is_some(), "Expected P011 diagnostic for reserved name");
    }

    #[test]
    fn wrap_in_braces_guard() {
        // Test for P016 braceless guard
        let source = "? true true";
        let diagnostics = check(source, None);
        // Should have parse error for missing braces
        let diagnostic = diagnostics.iter().find(|d| d.severity == Severity::Error);
        assert!(diagnostic.is_some(), "Expected error diagnostic for guard");
    }

    #[test]
    fn fix_p009_let_basic() {
        // P009 is the paren-form call error (`f(a, b)` needs commas).
        // Postfix form `f a b` is what ilo actually uses.
        let source = "f a:n>n;a(1 2)";
        let diagnostics = check(source, None);
        let diagnostic = diagnostics.iter().find(|d| d.code == Some("ILO-P009"));
        assert!(diagnostic.is_some(), "Expected P009 diagnostic for paren-form call");
    }

    #[test]
    fn fix_plan_rewrites_ternary_bool_literals() {
        let source = "is-valid-url s:t>b;? (& (contains s \"://\") (& (contains s \".\") (>= (len s) 10))) 1 0";
        let diagnostics = check(source, None);
        let diagnostic = diagnostics
            .iter()
            .find(|d| d.code == Some("ILO-T008")
                && d.suggestion.as_deref() == Some(crate::diagnostic::FIX_HINT_TERNARY_BOOL_LITERALS))
            .unwrap();
        let plan = diagnostic.fix_plan.as_ref().expect("ternary bool-literal fix");
        assert!(plan.edits[0].after.ends_with("1 0") == false);
        assert!(plan.edits[0].after.contains("true") && plan.edits[0].after.contains("false"));
        let fixed = repair_safe(source, None, 5);
        assert_eq!(fixed["ok"], true, "{fixed}");
    }

    #[test]
    fn fix_plan_rewrites_boolean_not_call_to_prefix_operator() {
        let source = "f flag:b>b;not flag";
        let diagnostics = check(source, None);
        let diagnostic = diagnostics.iter().find(|d| d.code == Some("ILO-T005")).unwrap();
        let plan = diagnostic.fix_plan.as_ref().expect("boolean-typed not fix");
        assert_eq!(plan.edits[0].before, "not flag");
        assert_eq!(plan.edits[0].after, "!flag");
        let fixed = repair_safe(source, None, 5);
        assert_eq!(fixed["ok"], true, "{fixed}");
        assert_eq!(fixed["fixed_source"], "f flag:b>b;!flag");

        let preview = repair_safe("f flag:b>b;not flag", None, 0);
        assert_eq!(preview["applied"], 0);
        assert!(preview["fixed_source"].is_null());
        assert_eq!(preview["preview"][0]["after"], "!flag");

        let unproven = repair_safe("f n:n>b;not n", None, 5);
        assert_eq!(unproven["applied"], 0, "{unproven}");
        assert!(unproven["fixed_source"].is_null());
    }

    #[test]
    fn fix_plan_swaps_at_arguments_only_when_types_prove_reversal() {
        let source = "f xs:L n>n;at 0 xs";
        let diagnostics = check(source, None);
        let diagnostic = diagnostics.iter().find(|d| d.code == Some("ILO-T013")
            && d.suggestion.as_deref() == Some(crate::diagnostic::FIX_HINT_SWAP_AT_ARGS)).unwrap();
        let plan = diagnostic.fix_plan.as_ref().expect("type-proven at swap");
        assert_eq!(plan.edits[0].before, "at 0 xs");
        assert_eq!(plan.edits[0].after, "at xs 0");
        let fixed = repair(source, None, 10);
        assert_eq!(fixed["ok"], true, "{fixed}");
        assert_eq!(fixed["fixed_source"], "f xs:L n>n;at xs 0");
    }

    #[test]
    fn reversed_at_fix_handles_assignments_and_binary_indices() {
        let source = "f xs:L n>n;first=at 0 xs;first";
        let diagnostics = check(source, None);
        let diagnostic = diagnostics
            .iter()
            .find(|d| {
                d.code == Some("ILO-T013")
                    && d.suggestion.as_deref()
                        == Some(crate::diagnostic::FIX_HINT_SWAP_AT_ARGS)
            })
            .unwrap();
        let plan = diagnostic.fix_plan.as_ref().expect("assignment at fix");
        assert_eq!(plan.edits[0].before, "first=at 0 xs");
        assert_eq!(plan.edits[0].after, "first=at xs 0");
        let fixed = repair_safe(source, None, 5);
        assert_eq!(fixed["ok"], true, "{fixed}");
        assert_eq!(fixed["fixed_source"], "f xs:L n>n;first=at xs 0;first");

        assert_eq!(
            crate::diagnostic::swap_at_args("at 0 (rev xs)"),
            Some("at (rev xs) 0".to_string())
        );
        assert_eq!(
            crate::diagnostic::swap_at_args("at - (len xs) 1 xs"),
            Some("at xs - (len xs) 1".to_string())
        );
        assert_eq!(
            crate::diagnostic::swap_at_args(">a b (at 0 (drop 1 xs)))"),
            Some(">a b (at (drop 1 xs) 0))".to_string())
        );
        assert_eq!(
            crate::diagnostic::swap_at_args("at 0 [[4 6] [6 8]]"),
            Some("at [[4 6] [6 8]] 0".to_string())
        );
        assert_eq!(
            crate::diagnostic::swap_at_args("at +i 1 xs"),
            None,
            "unparenthesized ambiguous operands must not be guessed"
        );
    }

    #[test]
    fn relative_imports_use_source_path() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("helper.ilo"), "double x:n>n;*x 2\n").unwrap();
        let path = directory.path().join("main.ilo");
        let source = "use \"./helper.ilo\"\nmain>n;double 3\n";
        std::fs::write(&path, source).unwrap();
        let diagnostics = check(source, path.to_str());
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let inline = check(source, None);
        assert!(inline.iter().any(|d| d.code == Some("ILO-P017")));
    }

}
