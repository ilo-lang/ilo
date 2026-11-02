//! Agent-oriented semantic queries built directly on ilo's compiler AST.

use crate::ast::{self, Decl, Program, Span};
use crate::codegen::fmt::{self, FmtMode};
use crate::{graph, lexer, parser};
use serde_json::{json, Value};

pub fn parse_program(source: &str) -> Result<Program, String> {
    let tokens = lexer::lex(source).map_err(|e| format!("{}: {}", e.code, e.suggestion))?;
    let token_spans = tokens
        .into_iter()
        .map(|(token, range)| {
            (
                token,
                Span {
                    start: range.start,
                    end: range.end,
                },
            )
        })
        .collect();
    let (mut program, errors) = parser::parse(token_spans);
    ast::resolve_aliases(&mut program);
    ast::desugar_dot_var_index(&mut program);
    program.source = Some(source.to_string());
    if let Some(error) = errors.first() {
        return Err(format!("{}: {}", error.code, error.message));
    }
    Ok(program)
}

pub fn inspect(source: &str, operation: &str, symbol: Option<&str>, budget: usize) -> Result<Value, String> {
    let program = parse_program(source)?;
    let graph = graph::build_graph(&program);

    match operation {
        "overview" => serde_json::to_value(graph).map_err(|e| e.to_string()),
        "symbol" => inspect_symbol(&program, &graph, required_symbol(symbol)?),
        "dependencies" => {
            let name = required_symbol(symbol)?;
            graph::query_budget(&program, &graph, name, budget)
                .map(|result| serde_json::to_value(result).map_err(|e| e.to_string()))
                .transpose()?
                .ok_or_else(|| format!("unknown function '{name}'"))
        }
        "callers" => {
            let name = required_symbol(symbol)?;
            graph::query_reverse(&program, &graph, name)
                .map(|result| serde_json::to_value(result).map_err(|e| e.to_string()))
                .transpose()?
                .ok_or_else(|| format!("unknown function '{name}'"))
        }
        _ => Err(format!(
            "unknown inspect operation '{}'; expected overview, symbol, dependencies, or callers",
            operation
        )),
    }
}

fn required_symbol(symbol: Option<&str>) -> Result<&str, String> {
    symbol.filter(|name| !name.is_empty()).ok_or_else(|| "symbol is required for this operation".to_string())
}

fn inspect_symbol(program: &Program, graph: &graph::ProgramGraph, name: &str) -> Result<Value, String> {
    if let Some(markdown) = super::builtins::hover_markdown(name) {
        return Ok(json!({
            "kind": "builtin",
            "name": name,
            "signature": super::builtins::signature_label(name),
            "aliases": super::builtins::spellings(name),
            "documentation": markdown
        }));
    }

    if let Some(query) = graph::query_fn(program, graph, name) {
        return serde_json::to_value(query).map_err(|e| e.to_string());
    }

    let decl = program.declarations.iter().find(|decl| declaration_name(decl) == Some(name));
    match decl {
        Some(decl) => Ok(json!({
            "kind": declaration_kind(decl),
            "name": name,
            "source": fmt::format_decl(decl, FmtMode::Dense)
        })),
        None => Err(format!("unknown symbol '{}'", name)),
    }
}

fn declaration_name(decl: &Decl) -> Option<&str> {
    match decl {
        Decl::Function { name, .. }
        | Decl::TypeDef { name, .. }
        | Decl::Tool { name, .. }
        | Decl::Alias { name, .. }
        | Decl::SumType { name, .. } => Some(name),
        Decl::Use { .. } | Decl::VersionPragma { .. } | Decl::Error { .. } => None,
    }
}

fn declaration_kind(decl: &Decl) -> &'static str {
    match decl {
        Decl::Function { .. } => "function",
        Decl::TypeDef { .. } => "type",
        Decl::Tool { .. } => "tool",
        Decl::Alias { .. } => "alias",
        Decl::SumType { .. } => "sum_type",
        Decl::Use { .. } => "import",
        Decl::VersionPragma { .. } => "version",
        Decl::Error { .. } => "error",
    }
}
