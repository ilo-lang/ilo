//! CLI argument parsing shared between the `ilo` binary and the AOT runtime.
//!
//! The binary entry (`src/main.rs`) calls `parse_cli_arg_for_param` per arg to
//! coerce raw shell strings into typed `runtime::Value`s before handing
//! them to the tree-walker / VM / JIT. The AOT runtime (`src/vm/mod.rs`)
//! re-uses the same logic via the C-callable helper `ilo_aot_parse_arg_list`
//! so that `main args:L t` binds the argv list the same way across engines.
//!
//! Keeping the implementation in one place avoids the silent
//! tree/VM-vs-AOT divergence that triggered the 0.12.1 fix: previously the
//! AOT entry shim treated `args:L t` as a single string and `len args`
//! returned the character count instead of the list length.
//!
//! See `src/main.rs::parse_cli_args_typed` for the binary-side path and
//! `tests/regression_aot_main_argv.rs` for the cross-engine coverage.

use crate::ast;
use crate::runtime;

/// Split a string on top-level commas, respecting `[]` nesting.
/// Used by `parse_cli_arg` so that nested-list literals like
/// `[[1,2],[3,4]]` are parsed as two elements rather than four.
pub fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0usize;
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'[' => depth += 1,
            b']' => depth -= 1,
            b',' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

/// Parse a single CLI arg without type guidance.
///
/// Recognises bracketed list literals (`[1,2,3]`), quoted strings, bare
/// comma lists, `nil`, numeric literals, and `true`/`false`. Everything
/// else becomes `Text`.
pub fn parse_cli_arg(s: &str) -> runtime::Value {
    if s.starts_with('[') && s.ends_with(']') {
        let inner = s[1..s.len() - 1].trim();
        if inner.is_empty() {
            return runtime::Value::List(std::sync::Arc::new(vec![]));
        }
        let items: Vec<runtime::Value> = split_top_level_commas(inner)
            .into_iter()
            .map(|part| parse_cli_arg(part.trim()))
            .collect();
        return runtime::Value::List(std::sync::Arc::new(items));
    }
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        return runtime::Value::Text(std::sync::Arc::new(s[1..s.len() - 1].to_string()));
    }
    if s.contains(',') {
        let items: Vec<runtime::Value> = split_top_level_commas(s)
            .into_iter()
            .map(|part| parse_cli_arg(part.trim()))
            .collect();
        return runtime::Value::List(std::sync::Arc::new(items));
    }
    if s == "nil" {
        return runtime::Value::Nil;
    }
    if let Ok(n) = s.parse::<f64>()
        && n.is_finite()
    {
        return runtime::Value::Number(n);
    }
    if s == "true" {
        runtime::Value::Bool(true)
    } else if s == "false" {
        runtime::Value::Bool(false)
    } else {
        runtime::Value::Text(std::sync::Arc::new(s.to_string()))
    }
}

/// Parse a single CLI arg, taking the declared param type into account.
///
/// For `Type::Text` params, the raw CLI string is kept verbatim as `Text`,
/// bypassing the greedy number/bool/list/nil parsing that `parse_cli_arg`
/// would otherwise perform. This prevents silent type drift where a function
/// declared as `arg:t` receives a `Number` at runtime because the CLI value
/// happened to look numeric (e.g. `"2"`), which then breaks `num arg` /
/// pattern-matches that key off the declared text type.
///
/// For every other declared type (or when no type is known), behaviour is
/// identical to `parse_cli_arg`.
pub fn parse_cli_arg_for_param(s: &str, expected: Option<&ast::Type>) -> runtime::Value {
    if matches!(expected, Some(ast::Type::Text)) {
        let stripped = if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
            &s[1..s.len() - 1]
        } else {
            s
        };
        return runtime::Value::Text(std::sync::Arc::new(stripped.to_string()));
    }
    parse_cli_arg(s)
}

/// Parse a single CLI arg known to be bound to a `L _` parameter.
///
/// Mirrors the binary-side coercion in `parse_cli_args_typed`: if the parsed
/// value isn't already a `List`, wrap it as `[value]`. This is the canonical
/// path for `main args:L t` across every engine.
pub fn parse_cli_arg_as_list(s: &str) -> runtime::Value {
    let v = parse_cli_arg(s);
    if matches!(v, runtime::Value::List(_)) {
        v
    } else {
        runtime::Value::List(std::sync::Arc::new(vec![v]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::Value;

    fn n(v: &Value) -> f64 {
        if let Value::Number(x) = v {
            *x
        } else {
            panic!("not number: {v:?}")
        }
    }
    fn t(v: &Value) -> String {
        if let Value::Text(s) = v {
            (**s).clone()
        } else {
            panic!("not text: {v:?}")
        }
    }
    fn list(v: &Value) -> std::sync::Arc<Vec<Value>> {
        if let Value::List(xs) = v {
            xs.clone()
        } else {
            panic!("not list: {v:?}")
        }
    }

    #[test]
    fn split_top_level_commas_basic() {
        assert_eq!(split_top_level_commas("a,b,c"), vec!["a", "b", "c"]);
    }

    #[test]
    fn split_top_level_commas_respects_brackets() {
        assert_eq!(
            split_top_level_commas("[1,2],[3,4]"),
            vec!["[1,2]", "[3,4]"]
        );
    }

    #[test]
    fn split_top_level_commas_no_split() {
        assert_eq!(split_top_level_commas("solo"), vec!["solo"]);
    }

    #[test]
    fn parse_cli_arg_number() {
        assert_eq!(n(&parse_cli_arg("42")), 42.0);
        assert_eq!(n(&parse_cli_arg("3.5")), 3.5);
        assert_eq!(n(&parse_cli_arg("-1.25")), -1.25);
    }

    #[test]
    fn parse_cli_arg_nil() {
        assert!(matches!(parse_cli_arg("nil"), Value::Nil));
    }

    #[test]
    fn parse_cli_arg_bools() {
        assert!(matches!(parse_cli_arg("true"), Value::Bool(true)));
        assert!(matches!(parse_cli_arg("false"), Value::Bool(false)));
    }

    #[test]
    fn parse_cli_arg_bare_text() {
        assert_eq!(t(&parse_cli_arg("hello")), "hello");
    }

    #[test]
    fn parse_cli_arg_quoted_text_keeps_inner() {
        assert_eq!(t(&parse_cli_arg("\"hi there\"")), "hi there");
    }

    #[test]
    fn parse_cli_arg_bracketed_empty_list() {
        let v = parse_cli_arg("[]");
        assert!(list(&v).is_empty());
    }

    #[test]
    fn parse_cli_arg_bracketed_list_with_spaces() {
        let v = parse_cli_arg("[a, b, c]");
        let xs = list(&v);
        assert_eq!(xs.len(), 3);
        assert_eq!(t(&xs[0]), "a");
        assert_eq!(t(&xs[2]), "c");
    }

    #[test]
    fn parse_cli_arg_nested_bracketed_list() {
        let v = parse_cli_arg("[[1,2],[3,4]]");
        let xs = list(&v);
        assert_eq!(xs.len(), 2);
        let inner = list(&xs[0]);
        assert_eq!(n(&inner[0]), 1.0);
        assert_eq!(n(&inner[1]), 2.0);
    }

    #[test]
    fn parse_cli_arg_bare_comma_list() {
        let v = parse_cli_arg("1,2,3");
        let xs = list(&v);
        assert_eq!(xs.len(), 3);
        assert_eq!(n(&xs[0]), 1.0);
    }

    #[test]
    fn parse_cli_arg_for_param_text_keeps_numeric_string() {
        let v = parse_cli_arg_for_param("2", Some(&ast::Type::Text));
        assert_eq!(t(&v), "2");
    }

    #[test]
    fn parse_cli_arg_for_param_text_strips_quotes() {
        let v = parse_cli_arg_for_param("\"hi\"", Some(&ast::Type::Text));
        assert_eq!(t(&v), "hi");
    }

    #[test]
    fn parse_cli_arg_for_param_no_hint_falls_through() {
        let v = parse_cli_arg_for_param("42", None);
        assert_eq!(n(&v), 42.0);
    }

    #[test]
    fn parse_cli_arg_for_param_non_text_hint_uses_default() {
        let v = parse_cli_arg_for_param("42", Some(&ast::Type::Number));
        assert_eq!(n(&v), 42.0);
    }

    #[test]
    fn parse_cli_arg_as_list_wraps_scalar() {
        let v = parse_cli_arg_as_list("hello");
        let xs = list(&v);
        assert_eq!(xs.len(), 1);
        assert_eq!(t(&xs[0]), "hello");
    }

    #[test]
    fn parse_cli_arg_as_list_wraps_number() {
        let v = parse_cli_arg_as_list("7");
        let xs = list(&v);
        assert_eq!(xs.len(), 1);
        assert_eq!(n(&xs[0]), 7.0);
    }

    #[test]
    fn parse_cli_arg_as_list_passes_through_bracketed() {
        let v = parse_cli_arg_as_list("[a,b,c]");
        let xs = list(&v);
        assert_eq!(xs.len(), 3);
    }

    #[test]
    fn parse_cli_arg_as_list_passes_through_bare_comma() {
        let v = parse_cli_arg_as_list("1,2,3");
        let xs = list(&v);
        assert_eq!(xs.len(), 3);
        assert_eq!(n(&xs[2]), 3.0);
    }

    #[test]
    fn parse_cli_arg_as_list_empty_brackets() {
        let v = parse_cli_arg_as_list("[]");
        assert!(list(&v).is_empty());
    }
}
