//! MCP language-intelligence adapter (newline-delimited JSON-RPC over stdio).
//!
//! `ilo lsp` exposes the compiler's existing parse/verify/fix pipeline as MCP
//! tools. The protocol shell is intentionally thin; `lang_svc` owns the logic.

use serde_json::{json, Value};

// ── Tools ──────────────────────────────────────────────────────────────────

fn validate(source: &str, path: Option<&str>, include_effects: bool) -> Value {
    let diagnostics = ilo::lang_svc::check(source, path);
    let ok = !diagnostics
        .iter()
        .any(|d| d.severity == ilo::diagnostic::Severity::Error);
    let mut out = json!({
        "ok": ok,
        "diagnostics": diagnostics.iter().map(ilo::lang_svc::diagnostic_json).collect::<Vec<_>>(),
    });
    if include_effects {
        out["effects"] = json!(ilo::lang_svc::inferred_effects(source, path)
            .into_iter()
            .map(|e| json!({"name": e.name, "declared": e.declared, "inferred": e.inferred}))
            .collect::<Vec<_>>());
    }
    tool_success(out)
}

fn fix(source: &str, path: Option<&str>, max_iters: usize, safe_only: bool) -> Value {
    let result = if safe_only {
        ilo::lang_svc::repair_safe(source, path, max_iters)
    } else {
        ilo::lang_svc::repair(source, path, max_iters)
    };
    tool_success(result)
}

fn apply_fix(source: &str, path: Option<&str>, index: usize) -> Value {
    tool_success(ilo::lang_svc::apply_one(source, path, index))
}

// ── Tool schema / dispatch ─────────────────────────────────────────────────

pub fn definitions() -> Value {
    json!({ "tools": tool_definitions() })
}

fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": "ilo_validate",
            "description": "Validate ilo source with the authoritative lexer/parser/verifier. Returns diagnostics with fix plans and optional inferred effects.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {"type": "string", "description": "Complete ilo source text"},
                    "path": {"type": "string", "description": "Optional filesystem path for imports and fix-plan paths"},
                    "effects": {"type": "boolean", "description": "Include inferred function effects"}
                },
                "required": ["source"]
            }
        }),
        json!({
            "name": "ilo_fix",
            "description": "Apply mechanical fix plans in a repair loop. safe_only=true restricts to type-proven rewrites; preview=true is a dry run.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {"type": "string"},
                    "path": {"type": "string"},
                    "max_iters": {"type": "integer", "minimum": 0, "maximum": 10},
                    "preview": {"type": "boolean"},
                    "safe_only": {"type": "boolean"}
                },
                "required": ["source"]
            }
        }),
        json!({
            "name": "ilo_apply_fix",
            "description": "Apply one fix-plan edit by index. Lowest-latency path when ilo_validate already reported fixable diagnostics.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": {"type": "string"},
                    "path": {"type": "string"},
                    "index": {"type": "integer", "minimum": 0}
                },
                "required": ["source"]
            }
        }),
    ]
}

pub fn call(name: &str, args: &Value) -> Value {
    let source = match args.get("source").and_then(Value::as_str) {
        Some(s) => s,
        None => return tool_error("source is required"),
    };
    let path = args.get("path").and_then(Value::as_str);
    match name {
        "ilo_validate" => validate(
            source,
            path,
            args.get("effects").and_then(Value::as_bool).unwrap_or(false),
        ),
        "ilo_fix" => {
            let preview = args.get("preview").and_then(Value::as_bool).unwrap_or(false);
            let max_iters = if preview {
                0
            } else {
                args.get("max_iters")
                    .and_then(Value::as_u64)
                    .and_then(|n| usize::try_from(n).ok())
                    .unwrap_or(10)
                    .min(10)
            };
            let safe_only = args
                .get("safe_only")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            fix(source, path, max_iters, safe_only)
        }
        "ilo_apply_fix" => {
            let index = args.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            apply_fix(source, path, index)
        }
        _ => tool_error(format!("unknown tool: {name}")),
    }
}

// ── MCP framing ────────────────────────────────────────────────────────────

fn tool_success(structured: Value) -> Value {
    let text =
        serde_json::to_string_pretty(&structured).unwrap_or_else(|_| structured.to_string());
    json!({
        "content": [{"type": "text", "text": text}],
        "structuredContent": structured,
        "isError": false
    })
}

fn tool_error(message: impl Into<String>) -> Value {
    json!({"content": [{"type": "text", "text": message.into()}], "isError": true})
}

fn rpc_result(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn rpc_error(id: Value, code: i64, message: String) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn dispatch(request: &Value) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    match method {
        "initialize" => rpc_result(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "ilo", "version": env!("CARGO_PKG_VERSION")}
            }),
        ),
        "ping" => rpc_result(id, json!({})),
        "tools/list" => rpc_result(id, definitions()),
        "tools/call" => {
            let params = request.get("params").cloned().unwrap_or(Value::Null);
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
            rpc_result(id, call(name, &arguments))
        }
        _ => rpc_error(id, -32601, format!("method not found: {method}")),
    }
}

/// Run the MCP server on stdio (newline-delimited JSON-RPC).
pub fn run() {
    use std::io::{self, BufRead, Write};
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                let err = rpc_error(Value::Null, -32700, format!("parse error: {e}"));
                let _ = writeln!(stdout, "{err}");
                let _ = stdout.flush();
                continue;
            }
        };
        // Notifications (no id) get no response.
        if request.get("id").is_none() {
            continue;
        }
        let response = dispatch(&request);
        let _ = writeln!(stdout, "{response}");
        let _ = stdout.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_definitions_match_implementations() {
        let defs = tool_definitions();
        let names: Vec<_> = defs.iter().map(|d| d["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["ilo_validate", "ilo_fix", "ilo_apply_fix"]);
    }

    #[test]
    fn validate_reports_fix_plan() {
        let res = call("ilo_validate", &json!({"source": r#"f xs:L t>t;join xs " ""#}));
        assert_eq!(res["isError"], false, "{res}");
        let body: Value = serde_json::from_str(res["content"][0]["text"].as_str().unwrap()).unwrap();
        let diag = body["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["code"] == "ILO-T005")
            .unwrap();
        assert!(diag["fix"]["edits"][0]["after"]
            .as_str()
            .unwrap()
            .starts_with("cat xs"));
    }

    #[test]
    fn fix_safe_only_rewrites_boolean_not() {
        let res = call(
            "ilo_fix",
            &json!({"source": "f flag:b>b;not flag", "safe_only": true, "max_iters": 5}),
        );
        let body: Value = serde_json::from_str(res["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(body["ok"], true, "{body}");
        assert_eq!(body["fixed_source"], "f flag:b>b;!flag");
    }

    #[test]
    fn initialize_has_tools_capability() {
        let response = dispatch(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"}));
        assert_eq!(response["id"], 1);
        assert!(response["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn unknown_method_returns_jsonrpc_error() {
        let response = dispatch(&json!({"id": 9, "method": "nope"}));
        assert_eq!(response["error"]["code"], -32601);
    }
}
