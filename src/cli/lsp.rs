//! MCP language intelligence adapter (newline-delimited JSON-RPC over stdio).
//!
//! Command: `ilo lsp`
//!
//! Note: Despite the name, this implements MCP (Model Context Protocol),
//! not the standard LSP (Language Server Protocol).

use serde_json::{json, Value};

// ── MCP Tools ──────────────────────────────────────────────────────────────

/// Validate source code with the core language services.
fn validate(source: &str, path: Option<&str>, include_effects: bool) -> Value {
    let diagnostics = ilo::lang_svc::check(source, path);
    let ok = !diagnostics.iter().any(|d| d.severity == ilo::diagnostic::Severity::Error);
    let mut structured = json!({
        "ok": ok,
        "diagnostics": diagnostics.iter().map(ilo::lang_svc::diagnostic_json).collect::<Vec<_>>()
    });

    if include_effects {
        let effects = ilo::lang_svc::inferred_effects(source, path)
            .into_iter()
            .map(|e| json!({
                "name": e.name,
                "declared": e.declared,
                "inferred": e.inferred
            }))
            .collect::<Vec<_>>();
        structured["effects"] = Value::Array(effects);
    }

    tool_success(structured)
}

/// Apply mechanical repair loop on source code.
fn fix(source: &str, path: Option<&str>, max_iters: usize, safe_only: bool) -> Value {
    let result = if safe_only {
        ilo::lang_svc::repair_safe(source, path, max_iters)
    } else {
        ilo::lang_svc::repair(source, path, max_iters)
    };
    tool_success(result)
}

/// Apply a single fix_plan edit by index (lowest-latency path).
fn apply_fix(source: &str, path: Option<&str>, index: usize) -> Value {
    tool_success(ilo::lang_svc::apply_one(source, path, index))
}

/// Semantic inspection (overview, symbol, callers, dependencies).
fn inspect(source: &str, operation: &str, symbol: Option<&str>, budget: usize) -> Value {
    match ilo::lang_svc::inspect(source, operation, symbol, budget) {
        Ok(result) => tool_success(result),
        Err(error) => tool_error(error),
    }
}

fn hover(source: &str, line: u32, character: u32) -> Value {
    match ilo::lang_svc::lsp_features::hover(source, line, character) {
        Some(v) => tool_success(v),
        None => tool_success(json!({"hover": null})),
    }
}

fn definition(source: &str, line: u32, character: u32) -> Value {
    match ilo::lang_svc::lsp_features::goto_definition(source, line, character) {
        Some(v) => tool_success(v),
        None => tool_success(json!({"definition": null})),
    }
}

fn completion(source: &str, line: u32, character: u32, prefix: Option<&str>) -> Value {
    let v = match prefix {
        Some(p) => ilo::lang_svc::lsp_features::completion_with_prefix(source, line, character, Some(p)),
        None => ilo::lang_svc::lsp_features::completion(source, line, character),
    };
    tool_success(v)
}

fn scaffold(interface: &str) -> Value {
    tool_success(ilo::lang_svc::lsp_features::scaffold(interface))
}

fn run_visible(source: &str, examples: &[String]) -> Value {
    tool_success(ilo::lang_svc::lsp_features::run_visible(source, examples))
}

fn symbols(source: &str) -> Value {
    tool_success(ilo::lang_svc::lsp_features::document_symbols(source))
}

fn format_src(source: &str, expanded: bool) -> Value {
    match ilo::lang_svc::lsp_features::format_source(source, expanded) {
        Ok(formatted) => tool_success(json!({"formatted": formatted})),
        Err(e) => tool_error(e),
    }
}

// ── Tool Helpers ───────────────────────────────────────────────────────────

fn extract_source(args: &Value) -> Option<&str> {
    args.get("source").and_then(Value::as_str)
}

fn extract_path(args: &Value) -> Option<&str> {
    args.get("path").and_then(Value::as_str)
}

fn extract_max_iters(args: &Value) -> usize {
    args.get("max_iters")
        .or_else(|| args.get("iterations"))
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(10)
}

fn extract_preview(args: &Value) -> bool {
    args.get("preview").and_then(Value::as_bool).unwrap_or(false)
}

fn extract_operation(args: &Value) -> Option<&str> {
    args.get("operation").and_then(Value::as_str)
}

fn extract_symbol(args: &Value) -> Option<&str> {
    args.get("symbol").and_then(Value::as_str)
}

fn extract_budget(args: &Value) -> usize {
    args.get("budget")
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(1200)
}

fn extract_includes_effects(args: &Value) -> bool {
    args.get("effects").and_then(Value::as_bool).unwrap_or(false)
}

fn extract_position(args: &Value) -> Option<(u32, u32)> {
    if let (Some(l), Some(c)) = (args.get("line").and_then(Value::as_u64), args.get("character").and_then(Value::as_u64)) {
        return Some((l as u32, c as u32));
    }
    if let Some(pos) = args.get("position") {
        let l = pos.get("line")?.as_u64()?;
        let c = pos.get("character")?.as_u64()?;
        return Some((l as u32, c as u32));
    }
    None
}

fn extract_expanded(args: &Value) -> bool {
    args.get("expanded").and_then(Value::as_bool).unwrap_or(false)
}

fn extract_interface(args: &Value) -> Option<&str> {
    args.get("interface").and_then(Value::as_str)
}

fn extract_examples(args: &Value) -> Option<Vec<String>> {
    let arr = args.get("examples")?.as_array()?;
    Some(arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
}

fn extract_prefix(args: &Value) -> Option<String> {
    args.get("prefix").and_then(Value::as_str).map(|s| s.to_string())
}

// ── Tool Definitions & Routing ─────────────────────────────────────────────

pub fn definitions() -> Value {
    json!({"tools": tool_definitions()})
}

fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "ilo_validate",
            "description": "Validate ilo source with the authoritative lexer, parser, import resolver, verifier, diagnostics, mechanical fix plans, and optional inferred effects.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {"type": "string", "description": "Complete ilo source text"},
                    "path": {"type": "string", "description": "Optional filesystem path for imports and fix plans"},
                    "effects": {"type": "boolean", "description": "Include inferred function effect sets"}
                },
                "required": ["source"]
            }
        }),
        json!({
            "name": "ilo_fix",
            "description": "Apply verifier fix plans in a repair loop. Set safe_only=true to apply only type-proven join→cat, boolean not→!, and reversed-at argument fixes; preview=true is a dry run.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {"type": "string", "description": "Complete ilo source text"},
                    "path": {"type": "string", "description": "Optional filesystem path for imports and fix plans"},
                    "max_iters": {"type": "integer", "minimum": 0, "maximum": 10},
                    "iterations": {"type": "integer", "minimum": 0, "maximum": 10},
                    "preview": {"type": "boolean"},
                    "safe_only": {"type": "boolean", "description": "Only apply type-proven join→cat, boolean not→!, and at-argument swap fixes"}
                },
                "required": ["source"]
            }
        }),
        json!({
            "name": "ilo_inspect",
            "description": "Inspect ilo semantics: overview, symbol, callers, or dependencies.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {"type": "string"},
                    "operation": {"type": "string", "enum": ["overview", "symbol", "dependencies", "callers"]},
                    "symbol": {"type": "string"},
                    "budget": {"type": "integer", "minimum": 1}
                },
                "required": ["source", "operation"]
            }
        }),
        json!({
            "name": "ilo_hover",
            "description": "Hover info for symbol at line/character. Returns markdown for builtins or user declarations.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {"type": "string"},
                    "line": {"type": "integer", "minimum": 0},
                    "character": {"type": "integer", "minimum": 0},
                    "position": {"type": "object", "properties": {"line": {"type": "integer"}, "character": {"type": "integer"}}}
                },
                "required": ["source"]
            }
        }),
        json!({
            "name": "ilo_definition",
            "description": "Go-to-definition for symbol at line/character. Returns declaration range.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {"type": "string"},
                    "line": {"type": "integer", "minimum": 0},
                    "character": {"type": "integer", "minimum": 0},
                    "position": {"type": "object", "properties": {"line": {"type": "integer"}, "character": {"type": "integer"}}}
                },
                "required": ["source"]
            }
        }),
        json!({
            "name": "ilo_complete",
            "description": "Completion items at line/character. Context-aware: after ':' returns type names, otherwise functions, types, builtins, keywords. MUST call before finishing if syntax uncertain.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {"type": "string"},
                    "line": {"type": "integer", "minimum": 0},
                    "character": {"type": "integer", "minimum": 0},
                    "position": {"type": "object", "properties": {"line": {"type": "integer"}, "character": {"type": "integer"}}},
                    "prefix": {"type": "string", "description": "Optional prefix filter; forwarded to completion ranking"}
                },
                "required": ["source"]
            }
        }),
        json!({
            "name": "ilo_symbols",
            "description": "Document symbols: list top-level declarations with kind and range.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {"type": "string"}
                },
                "required": ["source"]
            }
        }),
        json!({
            "name": "ilo_apply_fix",
            "description": "Apply a single fix_plan edit by index. Lowest-latency path when ilo_validate reports fix_plan. Returns {fixed_source, ok, remaining}. Use ilo_fix for multi-fix loops.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {"type": "string"},
                    "index": {"type": "integer", "minimum": 0, "description": "Zero-based index into fixable diagnostics (default 0)"},
                    "path": {"type": "string"}
                },
                "required": ["source"]
            }
        }),
        json!({
            "name": "ilo_scaffold",
            "description": "Return minimal valid ilo skeleton for given interface (e.g. 'taskfn xs:L n>L n'). Hyphenates params, body is { todo \"implement\" }. MUST use to start every solution.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "interface": {"type": "string", "description": "Interface string like 'taskfn xs:L n>L n'"}
                },
                "required": ["interface"]
            }
        }),
        json!({
            "name": "ilo_run_visible",
            "description": "Run visible examples given as input. Each example like 'taskfn [1 2 3] -> 6'. Returns {pass, detail}. Call after ilo_validate passes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {"type": "string"},
                    "examples": {"type": "array", "items": {"type": "string"}}
                },
                "required": ["source", "examples"]
            }
        }),
        json!({
            "name": "ilo_format",
            "description": "Format ilo source. Dense is wire format, expanded is human-readable.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {"type": "string"},
                    "expanded": {"type": "boolean", "description": "Use expanded (human) formatting; default is dense"}
                },
                "required": ["source"]
            }
        }),
    ]
}

pub fn call(name: &str, arguments: &Value) -> Value {
    match name {
        "ilo_validate" => {
            let Some(source) = extract_source(arguments) else {
                return tool_error("source is required");
            };
            let path = extract_path(arguments);
            let effects = extract_includes_effects(arguments);
            validate(source, path, effects)
        },
        "ilo_fix" => {
            let Some(source) = extract_source(arguments) else {
                return tool_error("source is required");
            };
            let path = extract_path(arguments);
            let max_iters = if extract_preview(arguments) {
                0
            } else {
                extract_max_iters(arguments)
            };
            let safe_only = arguments.get("safe_only").and_then(Value::as_bool).unwrap_or(false);
            fix(source, path, max_iters, safe_only)
        },
        "ilo_apply_fix" => {
            let Some(source) = extract_source(arguments) else {
                return tool_error("source is required");
            };
            let path = extract_path(arguments);
            let index = arguments.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            apply_fix(source, path, index)
        },
        "ilo_inspect" => {
            let Some(source) = extract_source(arguments) else {
                return tool_error("source is required");
            };
            let Some(operation) = extract_operation(arguments) else {
                return tool_error("operation is required");
            };
            let symbol = extract_symbol(arguments);
            let budget = extract_budget(arguments);
            inspect(source, operation, symbol, budget)
        },
        "ilo_hover" => {
            let Some(source) = extract_source(arguments) else {
                return tool_error("source is required");
            };
            let (line, character) = extract_position(arguments).unwrap_or((0, 0));
            hover(source, line, character)
        },
        "ilo_definition" => {
            let Some(source) = extract_source(arguments) else {
                return tool_error("source is required");
            };
            let (line, character) = extract_position(arguments).unwrap_or((0, 0));
            definition(source, line, character)
        },
        "ilo_complete" => {
            let Some(source) = extract_source(arguments) else {
                return tool_error("source is required");
            };
            let (line, character) = extract_position(arguments).unwrap_or((0, 0));
            let prefix = extract_prefix(arguments);
            completion(source, line, character, prefix.as_deref())
        },
        "ilo_scaffold" => {
            let Some(interface) = extract_interface(arguments) else {
                return tool_error("interface is required");
            };
            scaffold(interface)
        },
        "ilo_run_visible" => {
            let Some(source) = extract_source(arguments) else {
                return tool_error("source is required");
            };
            let Some(examples) = extract_examples(arguments) else {
                return tool_error("examples is required");
            };
            run_visible(source, &examples)
        },
        "ilo_symbols" => {
            let Some(source) = extract_source(arguments) else {
                return tool_error("source is required");
            };
            symbols(source)
        },
        "ilo_format" => {
            let Some(source) = extract_source(arguments) else {
                return tool_error("source is required");
            };
            let expanded = extract_expanded(arguments);
            format_src(source, expanded)
        },
        _ => tool_error(format!("unknown tool: {name}")),
    }
}

// ── Tool Response Helpers ──────────────────────────────────────────────────

fn tool_success(structured: Value) -> Value {
    let text = serde_json::to_string_pretty(&structured).unwrap_or_else(|_| structured.to_string());
    json!({
        "content": [{"type": "text", "text": text}],
        "structuredContent": structured,
        "isError": false
    })
}

fn tool_error(message: impl Into<String>) -> Value {
    let message = message.into();
    json!({
        "content": [{"type": "text", "text": message}],
        "isError": true
    })
}

// ── Protocol Transport ─────────────────────────────────────────────────────

pub fn run() {
    use std::io::{self, BufRead};
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() { continue; }
        let request = match serde_json::from_str::<Value>(line) {
            Ok(v) => v,
            Err(e) => {
                println!(
                    "{}",
                    serde_json::to_string(&json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": {"code": -32700, "message": format!("parse error: {e}")}
                    }))
                    .unwrap_or_else(|_| "{}".to_string())
                );
                continue;
            }
        };
        if request.get("id").is_none() { continue; }
        println!("{}", serde_json::to_string(&dispatch(&request)).unwrap_or_else(|_| "{}".to_string()));
    }
}

fn dispatch(request: &Value) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    match method {
        "initialize" => rpc_result(id, initialize_response()),
        "ping" => rpc_result(id, json!({})),
        "tools/list" => rpc_result(id, definitions()),
        "tools/call" => rpc_result(id, call(
            request.get("params")
                .and_then(|p| p.get("name"))
                .and_then(Value::as_str)
                .unwrap_or(""),
            request.get("params")
                .and_then(|p| p.get("arguments"))
                .unwrap_or(&Value::Null),
        )),
        _ => rpc_error(id, -32601, format!("method not found: {method}")),
    }
}

fn rpc_result(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn rpc_error(id: Value, code: i64, message: String) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn initialize_response() -> Value {
    json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {"tools": {}},
        "serverInfo": {
            "name": "ilo",
            "version": env!("CARGO_PKG_VERSION")
        }
    })
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_definitions_match_implementations() {
        let defs = tool_definitions();
        let names: Vec<_> = defs.iter().map(|d| d["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["ilo_validate", "ilo_fix", "ilo_inspect", "ilo_hover", "ilo_definition", "ilo_complete", "ilo_symbols", "ilo_apply_fix", "ilo_scaffold", "ilo_run_visible", "ilo_format"]);
    }

    #[test]
    fn validate_requires_source() {
        let res = call("ilo_validate", &json!({}));
        assert_eq!(res["isError"], true);
        assert!(res["content"][0]["text"].as_str().unwrap().contains("source"));
    }

    #[test]
    fn fix_requires_source() {
        let res = call("ilo_fix", &json!({}));
        assert_eq!(res["isError"], true);
    }

    #[test]
    fn validate_exposes_type_proven_join_quick_fix() {
        let res = call(
            "ilo_validate",
            &json!({"source": r#"f xs:L t>t;join xs " ""#}),
        );
        assert_eq!(res["isError"], false, "{res}");
        let body: Value = serde_json::from_str(res["content"][0]["text"].as_str().unwrap()).unwrap();
        let diagnostic = body["diagnostics"].as_array().unwrap().iter()
            .find(|d| d["code"] == "ILO-T005").unwrap();
        assert!(diagnostic["fix"]["edits"][0]["after"].as_str().unwrap().starts_with("cat xs"));
    }

    #[test]
    fn fix_safe_only_rewrites_boolean_not() {
        let res = call(
            "ilo_fix",
            &json!({"source": "f flag:b>b;not flag", "safe_only": true, "max_iters": 5}),
        );
        assert_eq!(res["isError"], false, "{res}");
        let body: Value = serde_json::from_str(res["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(body["ok"], true, "{body}");
        assert_eq!(body["fixed_source"], "f flag:b>b;!flag");
    }

    #[test]
    fn fix_safe_only_skips_generic_typo_renames() {
        let res = call(
            "ilo_fix",
            &json!({"source": "f x:n>n;unknow x", "safe_only": true, "max_iters": 5}),
        );
        assert_eq!(res["isError"], false, "{res}");
        let body: Value = serde_json::from_str(res["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(body["applied"], 0, "{body}");
        assert!(body["fixed_source"].is_null());
    }

    #[test]
    fn fix_safe_only_applies_proven_join_rewrite() {
        let res = call(
            "ilo_fix",
            &json!({"source": r#"f xs:L t>t;join xs " ""#, "safe_only": true, "max_iters": 5}),
        );
        assert_eq!(res["isError"], false, "{res}");
        let body: Value = serde_json::from_str(res["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(body["ok"], true, "{body}");
        assert_eq!(body["fixed_source"], r#"f xs:L t>t;cat xs " ""#);
    }

    #[test]
    fn fix_applies_the_new_at_argument_swap() {
        let res = call(
            "ilo_fix",
            &json!({"source": "f xs:L n>n;at 0 xs", "max_iters": 5}),
        );
        assert_eq!(res["isError"], false, "{res}");
        let body: Value = serde_json::from_str(res["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(body["ok"], true, "{body}");
        assert_eq!(body["fixed_source"], "f xs:L n>n;at xs 0");
    }

    #[test]
    fn inspect_requires_operation() {
        let res = call("ilo_inspect", &json!({"source": "test"}));
        assert_eq!(res["isError"], true);
        assert!(res["content"][0]["text"].as_str().unwrap().contains("operation"));
    }

    #[test]
    fn unknown_tool_error() {
        let res = call("unknown", &json!({}));
        assert_eq!(res["isError"], true);
    }

    #[test]
    fn initialize_has_rpc_envelope() {
        let response = dispatch(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"}));
        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 1);
        assert!(response["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn unknown_method_returns_error() {
        let response = dispatch(&json!({"id": 9, "method": "unknown"}));
        assert_eq!(response["id"], 9);
        assert_eq!(response["error"]["code"], -32601);
    }
}
