//! Protocol-neutral language services, shared by the CLI and MCP adapter.

mod fix;
#[doc(hidden)]
pub mod imports;

pub use fix::{apply_one, diagnostic_json, repair, repair_safe};

use crate::{ast, diagnostic::Diagnostic, lexer, parser, verify};

fn enrich(d: Diagnostic, source: &str, path: Option<&str>) -> Diagnostic {
    let mut d = d.with_source(source);
    if let Some(path) = path {
        d = d.with_path(path);
    }
    d.derive_fix_plan()
}

fn prepare(
    source: &str,
    path: Option<&str>,
) -> Result<(ast::Program, Vec<Diagnostic>), Diagnostic> {
    let tokens = lexer::lex(source).map_err(|e| enrich(Diagnostic::from(&e), source, path))?;
    let tokens = tokens
        .into_iter()
        .map(|(token, range)| (token, ast::Span { start: range.start, end: range.end }))
        .collect();
    let (mut program, errors) = parser::parse(tokens);
    ast::resolve_aliases(&mut program);
    ast::desugar_dot_var_index(&mut program);
    program.source = Some(source.to_string());
    let canonical = path.and_then(|p| std::path::Path::new(p).canonicalize().ok());
    let base_dir = canonical.as_ref().and_then(|p| p.parent());
    let mut visited = std::collections::HashSet::new();
    if let Some(canonical) = &canonical {
        visited.insert(canonical.clone());
    }
    let mut diagnostics = Vec::new();
    program.declarations = imports::resolve_imports(
        program.declarations,
        base_dir,
        &mut visited,
        &mut diagnostics,
        imports::BuildTarget::default(),
    );
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
    diagnostics.extend(
        result
            .warnings
            .iter()
            .chain(result.errors.iter())
            .map(|e| enrich(Diagnostic::from(e), source, path)),
    );
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
    fn check_valid_source() {
        let diags = check("x = 42", None);
        assert!(diags.iter().all(|d| d.severity != Severity::Error));
    }

    #[test]
    fn check_invalid_source_has_diagnostics() {
        let diags = check("f x:n>n;xyzz", None);
        assert!(diags.iter().any(|d| d.severity == Severity::Error));
    }
}
