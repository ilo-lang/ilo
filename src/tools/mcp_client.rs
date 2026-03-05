//! Low-level async MCP stdio client (JSON-RPC 2.0 over child process stdin/stdout).
//!
//! Spawns an MCP server as a child process and communicates via line-delimited
//! JSON-RPC over stdin/stdout. The child is killed when the client is dropped.

use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;

struct McpClientInner {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpClientInner {
    async fn send_request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let id = self.next_id;
        self.next_id += 1;

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let mut line = serde_json::to_string(&request)
            .map_err(|e| format!("MCP serialize error: {e}"))?;
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| format!("MCP write error: {e}"))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| format!("MCP flush error: {e}"))?;

        // Read response lines until we find one with a matching id.
        // Skip notifications (no "id" field) and unrelated messages.
        loop {
            let mut resp_line = String::new();
            let bytes = self
                .stdout
                .read_line(&mut resp_line)
                .await
                .map_err(|e| format!("MCP read error: {e}"))?;
            if bytes == 0 {
                return Err("MCP server closed connection".to_string());
            }
            let trimmed = resp_line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let resp: serde_json::Value = serde_json::from_str(trimmed)
                .map_err(|e| format!("MCP response parse error: {e}\nraw: {trimmed}"))?;

            // Match by numeric id
            if resp.get("id").and_then(|v| v.as_u64()) == Some(id) {
                if let Some(err) = resp.get("error") {
                    return Err(format!("MCP error response: {err}"));
                }
                return Ok(resp["result"].clone());
            }
            // Otherwise it's a notification or unrelated message — skip it
        }
    }

    async fn send_notification(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<(), String> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let mut line = serde_json::to_string(&notification)
            .map_err(|e| format!("MCP serialize error: {e}"))?;
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| format!("MCP write error: {e}"))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| format!("MCP flush error: {e}"))?;
        Ok(())
    }
}

/// Thread-safe MCP client. Wraps a child process with JSON-RPC communication.
pub struct McpClient {
    inner: Mutex<McpClientInner>,
    _child: Child,
}

impl McpClient {
    /// Spawn an MCP server and perform the initialize handshake.
    pub async fn connect(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
    ) -> Result<Self, String> {
        let mut cmd = tokio::process::Command::new(command);
        cmd.args(args);
        for (k, v) in env {
            cmd.env(k, v);
        }
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Failed to spawn MCP server '{command}': {e}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Failed to get MCP server stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Failed to get MCP server stdout".to_string())?;

        let mut inner = McpClientInner {
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
        };

        // Handshake: initialize
        inner
            .send_request(
                "initialize",
                serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": { "name": "ilo", "version": "0.4.0" }
                }),
            )
            .await?;

        // Notify that we are ready
        inner
            .send_notification("notifications/initialized", serde_json::json!({}))
            .await?;

        Ok(McpClient {
            inner: Mutex::new(inner),
            _child: child,
        })
    }

    /// List all tools exposed by this server. Returns raw JSON tool objects.
    pub async fn list_tools(&self) -> Result<Vec<serde_json::Value>, String> {
        let mut inner = self.inner.lock().await;
        let result = inner
            .send_request("tools/list", serde_json::json!({}))
            .await?;
        let tools = result["tools"]
            .as_array()
            .ok_or_else(|| "MCP tools/list: no 'tools' array in response".to_string())?
            .clone();
        Ok(tools)
    }

    /// Call a tool by name with named arguments JSON object.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let mut inner = self.inner.lock().await;
        inner
            .send_request(
                "tools/call",
                serde_json::json!({
                    "name": name,
                    "arguments": arguments,
                }),
            )
            .await
    }
}
