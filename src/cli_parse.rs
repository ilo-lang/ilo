//! CLI argument parsing shared between the `ilo` binary and the AOT runtime.
//!
//! The binary entry (`src/main.rs`) calls `parse_cli_arg_for_param` per arg to
//! coerce raw shell strings into typed `interpreter::Value`s before handing
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
use crate::interpreter;

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
pub fn parse_cli_arg(s: &str) -> interpreter::Value {
    if s.starts_with('[') && s.ends_with(']') {
        let inner = s[1..s.len() - 1].trim();
        if inner.is_empty() {
            return interpreter::Value::List(std::sync::Arc::new(vec![]));
        }
        let items: Vec<interpreter::Value> = split_top_level_commas(inner)
            .into_iter()
            .map(|part| parse_cli_arg(part.trim()))
            .collect();
        return interpreter::Value::List(std::sync::Arc::new(items));
    }
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        return interpreter::Value::Text(std::sync::Arc::new(s[1..s.len() - 1].to_string()));
    }
    if s.contains(',') {
        let items: Vec<interpreter::Value> = split_top_level_commas(s)
            .into_iter()
            .map(|part| parse_cli_arg(part.trim()))
            .collect();
        return interpreter::Value::List(std::sync::Arc::new(items));
    }
    if s == "nil" {
        return interpreter::Value::Nil;
    }
    if let Ok(n) = s.parse::<f64>()
        && n.is_finite()
    {
        return interpreter::Value::Number(n);
    }
    if s == "true" {
        interpreter::Value::Bool(true)
    } else if s == "false" {
        interpreter::Value::Bool(false)
    } else {
        interpreter::Value::Text(std::sync::Arc::new(s.to_string()))
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
pub fn parse_cli_arg_for_param(s: &str, expected: Option<&ast::Type>) -> interpreter::Value {
    if matches!(expected, Some(ast::Type::Text)) {
        let stripped = if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
            &s[1..s.len() - 1]
        } else {
            s
        };
        return interpreter::Value::Text(std::sync::Arc::new(stripped.to_string()));
    }
    parse_cli_arg(s)
}

/// Parse a single CLI arg known to be bound to a `L _` parameter.
///
/// Mirrors the binary-side coercion in `parse_cli_args_typed`: if the parsed
/// value isn't already a `List`, wrap it as `[value]`. This is the canonical
/// path for `main args:L t` across every engine.
pub fn parse_cli_arg_as_list(s: &str) -> interpreter::Value {
    let v = parse_cli_arg(s);
    if matches!(v, interpreter::Value::List(_)) {
        v
    } else {
        interpreter::Value::List(std::sync::Arc::new(vec![v]))
    }
}
