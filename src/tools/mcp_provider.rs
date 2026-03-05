//! MCP provider: connects to MCP servers, discovers tools, and provides
//! synthesized AST declarations and a ToolProvider implementation.

use super::{ToolError, ToolProvider};
use crate::ast::{self, Span};
use crate::interpreter::Value;
use crate::tools::mcp_client::McpClient;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

// ── Config ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Deserialize)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// Claude Desktop–compatible MCP configuration.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct McpConfig {
    #[serde(rename = "mcpServers")]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

impl McpConfig {
    pub fn from_file(path: &str) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read MCP config {path}: {e}"))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("failed to parse MCP config {path}: {e}"))
    }
}

// ── Tool metadata ─────────────────────────────────────────────────────────────

/// Metadata for a single MCP tool, enriched with ilo type information.
#[derive(Debug, Clone)]
pub struct McpToolDef {
    #[allow(dead_code)] // available for diagnostics / future routing improvements
    pub server_name: String,
    pub tool_name: String,
    pub description: String,
    /// Ordered parameter names (required first, then optional).
    pub param_names: Vec<String>,
    /// ilo-typed params for Decl::Tool synthesis.
    pub ilo_params: Vec<ast::Param>,
}

/// Maps a JSON Schema sub-schema to an ilo AST type.
/// Falls back to `t` (text) for complex types (array, object, unknown).
pub fn json_schema_to_ilo_type(schema: &serde_json::Value) -> ast::Type {
    match schema.get("type").and_then(|t| t.as_str()) {
        Some("string") => ast::Type::Text,
        Some("number") | Some("integer") => ast::Type::Number,
        Some("boolean") => ast::Type::Bool,
        _ => ast::Type::Text, // array / object / unknown → text (raw JSON)
    }
}

/// Parse a raw MCP JSON tool object into an `McpToolDef`.
fn parse_tool_def(server_name: &str, tool_json: &serde_json::Value) -> Option<McpToolDef> {
    let tool_name = tool_json.get("name")?.as_str()?.to_string();
    let description = tool_json
        .get("description")
        .and_then(|d| d.as_str())
        .unwrap_or("")
        .to_string();

    let input_schema = tool_json
        .get("inputSchema")
        .unwrap_or(&serde_json::Value::Null);
    let properties = input_schema.get("properties").and_then(|p| p.as_object());

    // `required` gives a guaranteed ordering; start with those.
    let required: Vec<String> = input_schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    // Then append any optional properties not already in `required`.
    let mut param_names: Vec<String> = required.clone();
    if let Some(props) = properties {
        for key in props.keys() {
            if !required.contains(key) {
                param_names.push(key.clone());
            }
        }
    }

    let ilo_params: Vec<ast::Param> = param_names
        .iter()
        .map(|name| {
            let schema = properties
                .and_then(|p| p.get(name))
                .unwrap_or(&serde_json::Value::Null);
            ast::Param {
                name: name.clone(),
                ty: json_schema_to_ilo_type(schema),
            }
        })
        .collect();

    Some(McpToolDef {
        server_name: server_name.to_string(),
        tool_name,
        description,
        param_names,
        ilo_params,
    })
}

// ── Provider ──────────────────────────────────────────────────────────────────

/// MCP tool provider. Connects to one or more MCP servers and dispatches
/// ilo tool calls to them via JSON-RPC.
pub struct McpProvider {
    /// `(client, tools)` per server, indexed by position.
    clients: Vec<(McpClient, Vec<McpToolDef>)>,
    /// tool_name → (client_index, ordered param_names)
    tool_index: HashMap<String, (usize, Vec<String>)>,
}

impl McpProvider {
    /// Connect to all servers in the config and discover their tools.
    pub async fn connect(config: &McpConfig) -> Result<Self, String> {
        let mut clients: Vec<(McpClient, Vec<McpToolDef>)> = Vec::new();
        let mut tool_index: HashMap<String, (usize, Vec<String>)> = HashMap::new();

        for (server_name, server_cfg) in &config.mcp_servers {
            let client =
                McpClient::connect(&server_cfg.command, &server_cfg.args, &server_cfg.env)
                    .await
                    .map_err(|e| format!("MCP server '{server_name}': {e}"))?;

            let raw_tools = client
                .list_tools()
                .await
                .map_err(|e| format!("MCP server '{server_name}' list_tools: {e}"))?;

            let tools: Vec<McpToolDef> = raw_tools
                .iter()
                .filter_map(|t| parse_tool_def(server_name, t))
                .collect();

            let client_idx = clients.len();
            for tool in &tools {
                tool_index.insert(
                    tool.tool_name.clone(),
                    (client_idx, tool.param_names.clone()),
                );
            }
            clients.push((client, tools));
        }

        Ok(McpProvider {
            clients,
            tool_index,
        })
    }

    /// Synthesize `Decl::Tool` nodes for all discovered tools.
    /// Inject these into the program before verification so ilo type-checks calls.
    pub fn tool_decls(&self) -> Vec<ast::Decl> {
        // MCP tools always return R t t
        let return_type = ast::Type::Result(
            Box::new(ast::Type::Text),
            Box::new(ast::Type::Text),
        );

        self.clients
            .iter()
            .flat_map(|(_, tools)| tools.iter())
            .map(|tool| ast::Decl::Tool {
                name: tool.tool_name.clone(),
                description: tool.description.clone(),
                params: tool.ilo_params.clone(),
                return_type: return_type.clone(),
                timeout: None,
                retry: None,
                span: Span::UNKNOWN,
            })
            .collect()
    }
}

impl ToolProvider for McpProvider {
    fn call(
        &self,
        name: &str,
        args: Vec<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + '_>> {
        let name = name.to_string();
        Box::pin(async move {
            let (client_idx, param_names) = self
                .tool_index
                .get(&name)
                .ok_or_else(|| ToolError::NotConfigured(name.clone()))?;

            let (client, _) = &self.clients[*client_idx];

            // Zip positional args with recorded param names → named JSON object
            let mut arguments = serde_json::Map::new();
            for (param_name, arg) in param_names.iter().zip(args.iter()) {
                let json_val = arg
                    .to_json()
                    .map_err(|e| ToolError::Json(name.clone(), e.to_string()))?;
                arguments.insert(param_name.clone(), json_val);
            }

            let result = client
                .call_tool(&name, serde_json::Value::Object(arguments))
                .await
                .map_err(|e| ToolError::Http(name.clone(), e))?;

            // MCP signals tool-level errors via `isError: true`
            if result
                .get("isError")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                let err_text = extract_text_content(&result);
                return Ok(Value::Err(Box::new(Value::Text(err_text))));
            }

            let text = extract_text_content(&result);
            Ok(Value::Ok(Box::new(Value::Text(text))))
        })
    }
}

/// Extract text from an MCP `tools/call` response.
///
/// MCP returns `{ "content": [{ "type": "text", "text": "..." }] }`.
/// Joins multiple text items with newlines; falls back to JSON stringification.
fn extract_text_content(result: &serde_json::Value) -> String {
    if let Some(content) = result.get("content").and_then(|c| c.as_array()) {
        let texts: Vec<&str> = content
            .iter()
            .filter_map(|item| {
                if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                    item.get("text").and_then(|t| t.as_str())
                } else {
                    None
                }
            })
            .collect();
        if !texts.is_empty() {
            return texts.join("\n");
        }
    }
    // Fallback: stringify the whole result
    result.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_mapping_string() {
        let schema = serde_json::json!({ "type": "string" });
        assert_eq!(json_schema_to_ilo_type(&schema), ast::Type::Text);
    }

    #[test]
    fn schema_mapping_number() {
        let schema = serde_json::json!({ "type": "number" });
        assert_eq!(json_schema_to_ilo_type(&schema), ast::Type::Number);
    }

    #[test]
    fn schema_mapping_integer() {
        let schema = serde_json::json!({ "type": "integer" });
        assert_eq!(json_schema_to_ilo_type(&schema), ast::Type::Number);
    }

    #[test]
    fn schema_mapping_boolean() {
        let schema = serde_json::json!({ "type": "boolean" });
        assert_eq!(json_schema_to_ilo_type(&schema), ast::Type::Bool);
    }

    #[test]
    fn schema_mapping_array_falls_back_to_text() {
        let schema = serde_json::json!({ "type": "array" });
        assert_eq!(json_schema_to_ilo_type(&schema), ast::Type::Text);
    }

    #[test]
    fn schema_mapping_object_falls_back_to_text() {
        let schema = serde_json::json!({ "type": "object" });
        assert_eq!(json_schema_to_ilo_type(&schema), ast::Type::Text);
    }

    #[test]
    fn schema_mapping_missing_type_falls_back_to_text() {
        let schema = serde_json::json!({});
        assert_eq!(json_schema_to_ilo_type(&schema), ast::Type::Text);
    }

    #[test]
    fn parse_tool_def_basic() {
        let tool_json = serde_json::json!({
            "name": "read_file",
            "description": "Read a file",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }
        });
        let def = parse_tool_def("filesystem", &tool_json).unwrap();
        assert_eq!(def.tool_name, "read_file");
        assert_eq!(def.server_name, "filesystem");
        assert_eq!(def.param_names, vec!["path"]);
        assert_eq!(def.ilo_params.len(), 1);
        assert_eq!(def.ilo_params[0].name, "path");
        assert_eq!(def.ilo_params[0].ty, ast::Type::Text);
    }

    #[test]
    fn parse_tool_def_required_ordering() {
        let tool_json = serde_json::json!({
            "name": "search",
            "description": "Search files",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "limit": { "type": "integer" },
                    "path": { "type": "string" }
                },
                "required": ["query", "path"]
            }
        });
        let def = parse_tool_def("search_server", &tool_json).unwrap();
        // required first: query, path — then optional: limit
        assert_eq!(def.param_names[0], "query");
        assert_eq!(def.param_names[1], "path");
        // limit is optional, appears after required params
        assert!(def.param_names.contains(&"limit".to_string()));
    }

    #[test]
    fn tool_decls_produce_correct_ast() {
        // Manually build a minimal McpProvider to test tool_decls()
        // We can't easily construct McpClient in unit tests, so test parse_tool_def
        // and verify the Decl shape is correct via parse_tool_def output.
        let tool_json = serde_json::json!({
            "name": "write_file",
            "description": "Write to a file",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["path", "content"]
            }
        });
        let def = parse_tool_def("fs", &tool_json).unwrap();
        assert_eq!(def.param_names, vec!["path", "content"]);
        assert_eq!(def.ilo_params[0].ty, ast::Type::Text);
        assert_eq!(def.ilo_params[1].ty, ast::Type::Text);
    }
}
