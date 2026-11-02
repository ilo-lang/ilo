//! LSP-oriented language features: hover, completion, go-to-def, symbols, formatting.
//! Ported from the prototype `ilo-lsp` (f158f01) and adapted to `crate::` paths.
//! Stateless: every call re-parses source, no document sync.

use crate::ast::{Decl, Program, Span, Type};
use crate::codegen::fmt::{self, FmtMode};
use crate::{lexer, parser, verify};
use serde_json::{json, Value};

// ── Position / span conversion ───────────────────────────────────────────

pub fn offset_to_position(source: &str, offset: usize) -> Value {
    let clamped = offset.min(source.len());
    let before = &source[..clamped];
    let line = before.bytes().filter(|&b| b == b'\n').count();
    let col = before.rfind('\n').map(|p| clamped - p - 1).unwrap_or(clamped);
    json!({"line": line, "character": col})
}

fn offset_to_lsp_pos(source: &str, offset: usize) -> (u32, u32) {
    let clamped = offset.min(source.len());
    let before = &source[..clamped];
    let line = before.bytes().filter(|&b| b == b'\n').count() as u32;
    let col = before.rfind('\n').map(|p| clamped - p - 1).unwrap_or(clamped) as u32;
    (line, col)
}

pub fn position_to_offset(source: &str, line: u32, character: u32) -> usize {
    let mut cur_line = 0u32;
    let mut offset = 0usize;
    for (i, c) in source.char_indices() {
        if cur_line == line {
            // remain on target line: advance by character within line
            let line_start = i;
            // find end of this line
            let line_end = source[line_start..].find('\n').map(|p| line_start + p).unwrap_or(source.len());
            let line_slice = &source[line_start..line_end];
            // character is UTF-16-ish but we treat as byte offset clamped
            let char_offset = (character as usize).min(line_slice.len());
            return line_start + char_offset;
        }
        if c == '\n' {
            cur_line += 1;
        }
        offset = i + c.len_utf8();
    }
    // past last line or on final line beyond iteration
    if cur_line == line {
        let last_newline = source.rfind('\n').map(|p| p + 1).unwrap_or(0);
        let line_slice = &source[last_newline..];
        return last_newline + (character as usize).min(line_slice.len());
    }
    offset.min(source.len())
}

pub fn span_to_range(source: &str, span: Span) -> Value {
    let (sl, sc) = offset_to_lsp_pos(source, span.start);
    let (el, ec) = offset_to_lsp_pos(source, span.end);
    json!({"start": {"line": sl, "character": sc}, "end": {"line": el, "character": ec}})
}

fn ident_at(source: &str, offset: usize) -> Option<(String, usize)> {
    if offset > source.len() {
        return None;
    }
    let bytes = source.as_bytes();
    let mut start = offset;
    while start > 0 {
        let b = bytes[start - 1];
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
            start -= 1;
        } else {
            break;
        }
    }
    let mut end = offset;
    while end < bytes.len() {
        let b = bytes[end];
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
            end += 1;
        } else {
            break;
        }
    }
    if start == end {
        return None;
    }
    Some((source[start..end].to_string(), start))
}

// ── Helpers ───────────────────────────────────────────────────────────────

const KEYWORDS: &[&str] = &[
    "type", "tool", "use", "with", "alias", "ret", "brk", "cnt", "wh", "true", "false", "nil",
];
const TYPE_NAMES: &[&str] = &["n", "t", "b", "_", "L", "R", "M", "S", "F", "O"];

fn parse_program(source: &str) -> Option<Program> {
    let tokens = lexer::lex(source).ok()?;
    let token_spans: Vec<_> = tokens
        .into_iter()
        .map(|(t, r)| (t, Span { start: r.start, end: r.end }))
        .collect();
    let (mut program, _) = parser::parse(token_spans);
    crate::ast::resolve_aliases(&mut program);
    crate::ast::desugar_dot_var_index(&mut program);
    program.source = Some(source.to_string());
    Some(program)
}

fn type_display(ty: &Type) -> String {
    crate::codegen::fmt::type_str(ty)
}

fn decl_hover(decl: &Decl) -> Option<String> {
    match decl {
        Decl::Function { name, params, return_type, .. } => {
            let params_str = params
                .iter()
                .map(|p| format!("{}:{}", p.name, type_display(&p.ty)))
                .collect::<Vec<_>>()
                .join(" ");
            Some(format!("**fn** `{}`\n\n```\n{} {}>{}\n```", name, name, params_str, type_display(return_type)))
        }
        Decl::TypeDef { name, fields, .. } => {
            let fields_str = fields
                .iter()
                .map(|f| format!("  {}:{}", f.name, type_display(&f.ty)))
                .collect::<Vec<_>>()
                .join("\n");
            Some(format!("**type** `{}`\n\n```\ntype {} {{\n{}\n}}\n```", name, name, fields_str))
        }
        Decl::Alias { name, target, .. } => Some(format!("**alias** `{}` = `{}`", name, type_display(target))),
        Decl::Tool { name, description, params, return_type, .. } => {
            let params_str = params
                .iter()
                .map(|p| format!("{}:{}", p.name, type_display(&p.ty)))
                .collect::<Vec<_>>()
                .join(" ");
            Some(format!("**tool** `{}`\n\n{}\n\n`{} {}>{}`", name, description, name, params_str, type_display(return_type)))
        }
        _ => None,
    }
}

// ── Public LSP features ───────────────────────────────────────────────────

/// Hover at position. Returns markdown + optional range.
pub fn hover(source: &str, line: u32, character: u32) -> Option<Value> {
    let offset = position_to_offset(source, line, character);
    let (ident, _) = ident_at(source, offset)?;
    // builtin first
    if let Some(md) = super::builtins::hover_markdown(&ident) {
        return Some(json!({
            "contents": {"kind": "markdown", "value": md},
            "ident": ident
        }));
    }
    // user decls
    let program = parse_program(source)?;
    for decl in &program.declarations {
        let name = match decl {
            Decl::Function { name, .. }
            | Decl::TypeDef { name, .. }
            | Decl::Tool { name, .. }
            | Decl::Alias { name, .. }
            | Decl::SumType { name, .. } => name.as_str(),
            _ => continue,
        };
        if name == ident {
            if let Some(text) = decl_hover(decl) {
                return Some(json!({
                    "contents": {"kind": "markdown", "value": text},
                    "ident": ident
                }));
            }
        }
    }
    None
}

/// Go-to-definition: locate declaration span for symbol under cursor.
pub fn goto_definition(source: &str, line: u32, character: u32) -> Option<Value> {
    let offset = position_to_offset(source, line, character);
    let (ident, _) = ident_at(source, offset)?;
    let program = parse_program(source)?;
    for decl in &program.declarations {
        let (name, span) = match decl {
            Decl::Function { name, span, .. } => (name.as_str(), *span),
            Decl::TypeDef { name, span, .. } => (name.as_str(), *span),
            Decl::Tool { name, span, .. } => (name.as_str(), *span),
            Decl::Alias { name, span, .. } => (name.as_str(), *span),
            Decl::SumType { name, span, .. } => (name.as_str(), *span),
            _ => continue,
        };
        if name == ident {
            return Some(json!({
                "name": name,
                "range": span_to_range(source, span),
                "ident": ident
            }));
        }
    }
    None
}

pub fn scaffold(interface: &str) -> Value {
    let iface = interface.trim().replace('_', "-");
    if iface.is_empty() {
        return json!({"scaffold": "taskfn x:n>n;{ todo \"implement\" }"});
    }
    let scaffold = format!("{};{{ todo \"implement\" }}", iface);
    json!({"scaffold": scaffold})
}

/// Completion items at position.
pub fn completion(source: &str, line: u32, character: u32) -> Value {
    completion_with_prefix(source, line, character, None)
}

pub fn completion_with_prefix(source: &str, line: u32, character: u32, prefix_opt: Option<&str>) -> Value {
    let offset = position_to_offset(source, line, character);
    let after_colon = offset > 0 && {
        let before = &source[..offset.min(source.len())];
        before.trim_end_matches(|c: char| c.is_alphanumeric() || c == '_' || c == '-').ends_with(':')
    };
    let mut items: Vec<Value> = Vec::new();
    if after_colon {
        for &ty in TYPE_NAMES {
            items.push(json!({"label": ty, "kind": "type_parameter"}));
        }
        return json!({"items": items, "context": "type"});
    }
    if let Some(program) = parse_program(source) {
        for decl in &program.declarations {
            match decl {
                Decl::Function { name, params, return_type, .. } => {
                    let params_str = params.iter().map(|p| format!("{}:{}", p.name, type_display(&p.ty))).collect::<Vec<_>>().join(" ");
                    items.push(json!({"label": name, "kind": "function", "detail": format!("{} {}>{}", name, params_str, type_display(return_type))}));
                }
                Decl::TypeDef { name, .. } => items.push(json!({"label": name, "kind": "class"})),
                Decl::Tool { name, description, .. } => items.push(json!({"label": name, "kind": "interface", "detail": description})),
                Decl::Alias { name, .. } => items.push(json!({"label": name, "kind": "type_parameter"})),
                Decl::SumType { name, .. } => items.push(json!({"label": name, "kind": "enum"})),
                _ => {}
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    for (name, _, _) in verify::BUILTINS {
        if !seen.insert(*name) { continue; }
        items.push(json!({"label": name, "kind": "function", "detail": super::builtins::signature_label(name)}));
    }
    for &kw in KEYWORDS {
        items.push(json!({"label": kw, "kind": "keyword"}));
    }
    let prefix = prefix_opt.map(|s| s.to_lowercase()).or_else(|| ident_at(source, offset).map(|(s, _)| s.to_lowercase()));
    if let Some(p) = prefix.as_deref().filter(|p| !p.is_empty()) {
        items.retain(|it| it.get("label").and_then(Value::as_str).map(|l| l.to_lowercase().starts_with(p)).unwrap_or(false));
    }
    if source.contains("L ") {
        let list_ops = ["len","at","lst","take","drop","slc","rev","srt","rsrt","unq","flat","grp","zip","range","map","flt","fld","ct","enumerate","chunks","window","flatmap","where","srt"];
        items.sort_by(|a,b| {
            let la = a.get("label").and_then(Value::as_str).unwrap_or("");
            let lb = b.get("label").and_then(Value::as_str).unwrap_or("");
            let pa = list_ops.contains(&la);
            let pb = list_ops.contains(&lb);
            pb.cmp(&pa)
        });
    }
    if items.len() > 50 { items.truncate(50); }
    json!({"items": items})
}

/// Document symbols (flat list).
pub fn document_symbols(source: &str) -> Value {
    let program = match parse_program(source) {
        Some(p) => p,
        None => return json!({"symbols": []}),
    };
    let symbols: Vec<Value> = program.declarations.iter().filter_map(|decl| {
        let (name, kind, span) = match decl {
            Decl::Function { name, span, .. } => (name.as_str(), "function", *span),
            Decl::TypeDef { name, span, .. } => (name.as_str(), "class", *span),
            Decl::Tool { name, span, .. } => (name.as_str(), "interface", *span),
            Decl::Alias { name, span, .. } => (name.as_str(), "type_parameter", *span),
            Decl::SumType { name, span, .. } => (name.as_str(), "enum", *span),
            _ => return None,
        };
        Some(json!({"name": name, "kind": kind, "range": span_to_range(source, span)}))
    }).collect();
    json!({"symbols": symbols})
}

pub fn run_visible(source: &str, examples: &[String]) -> Value {
    use crate::interpreter::Value as IVal;
    use std::sync::Arc;
    fn parse_ilo_val(s: &str) -> IVal {
        let t = s.trim();
        if t == "nil" { return IVal::Nil; }
        if t == "true" { return IVal::Bool(true); }
        if t == "false" { return IVal::Bool(false); }
        if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
            return IVal::Text(Arc::new(t[1..t.len()-1].to_string()));
        }
        if t.starts_with('[') && t.ends_with(']') {
            let inner = &t[1..t.len()-1].trim();
            if inner.is_empty() { return IVal::List(Arc::new(vec![])); }
            let parts = split_ilo_list(inner);
            let vals: Vec<IVal> = parts.iter().map(|p| parse_ilo_val(p)).collect();
            return IVal::List(Arc::new(vals));
        }
        if let Ok(n) = t.parse::<f64>() { return IVal::Number(n); }
        IVal::Text(Arc::new(t.to_string()))
    }
    fn split_ilo_list(inner: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut cur = String::new();
        let mut depth = 0i32;
        let mut in_str = false;
        for c in inner.chars() {
            if c == '"' { in_str = !in_str; cur.push(c); }
            else if in_str { cur.push(c); }
            else if c == '[' { depth += 1; cur.push(c); }
            else if c == ']' { depth -= 1; cur.push(c); }
            else if c.is_whitespace() && depth == 0 {
                if !cur.is_empty() { out.push(cur.clone()); cur.clear(); }
            } else { cur.push(c); }
        }
        if !cur.is_empty() { out.push(cur); }
        out
    }
    fn split_args(call: &str) -> Vec<String> {
        let t = call.trim();
        let after = if let Some(rest) = t.strip_prefix("taskfn") { rest.trim() } else { t };
        if after.is_empty() { return vec![]; }
        let mut args = Vec::new();
        let mut cur = String::new();
        let mut depth = 0i32;
        let mut in_str = false;
        for c in after.chars() {
            if c == '"' { in_str = !in_str; cur.push(c); }
            else if in_str { cur.push(c); }
            else if c == '[' { depth += 1; cur.push(c); }
            else if c == ']' { depth -= 1; cur.push(c); }
            else if c.is_whitespace() && depth == 0 && !in_str {
                if !cur.is_empty() { args.push(cur.clone()); cur.clear(); }
            } else { cur.push(c); }
        }
        if !cur.is_empty() { args.push(cur); }
        args
    }
    let diags = crate::lang_svc::check(source, None);
    let has_err = diags.iter().any(|d| d.severity == crate::diagnostic::Severity::Error);
    if has_err {
        return json!({"pass": false, "detail": "check failed", "diagnostics": diags.iter().map(crate::lang_svc::diagnostic_json).collect::<Vec<_>>()});
    }
    let program = match parse_program(source) {
        Some(p) => p,
        None => return json!({"pass": false, "detail": "parse failed"}),
    };
    let mut results = Vec::new();
    let mut all_pass = true;
    for ex in examples {
        let (left, right) = match ex.split_once("->") { Some((l,r)) => (l, r), None => { results.push(json!({"example": ex, "pass": false, "detail": "missing ->"})); all_pass = false; continue; } };
        let args_strs = split_args(left);
        let expected = parse_ilo_val(right);
        let args_vals: Vec<IVal> = args_strs.iter().map(|s| parse_ilo_val(s)).collect();
        let res = crate::interpreter::run(&program, Some("taskfn"), args_vals.clone());
        match res {
            Ok(v) => {
                let pass = v == expected;
                if !pass { all_pass = false; }
                results.push(json!({"example": ex, "pass": pass, "got": format!("{}", v), "expected": format!("{}", expected)}));
            }
            Err(e) => {
                all_pass = false;
                results.push(json!({"example": ex, "pass": false, "error": format!("{}", e), "got": null}));
            }
        }
    }
    json!({"pass": all_pass, "results": results, "detail": if all_pass { "all visible examples passed" } else { "some examples failed" }})
}

/// Format source. Returns formatted string or error.
pub fn format_source(source: &str, expanded: bool) -> Result<String, String> {
    let tokens = lexer::lex(source).map_err(|e| format!("{}: {}", e.code, e.suggestion))?;
    let token_spans = tokens.into_iter().map(|(t, r)| (t, Span{start: r.start, end: r.end})).collect();
    let (mut program, errors) = parser::parse(token_spans);
    if let Some(e) = errors.first() {
        return Err(format!("{}: {}", e.code, e.message));
    }
    crate::ast::resolve_aliases(&mut program);
    crate::ast::desugar_dot_var_index(&mut program);
    let mode = if expanded { FmtMode::Expanded } else { FmtMode::Dense };
    Ok(fmt::format(&program, mode))
}
