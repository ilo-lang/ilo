//! Low-level async MCP stdio client (JSON-RPC 2.0 over child process stdin/stdout).
//!
//! Spawns an MCP server as a child process and communicates via line-delimited
//! JSON-RPC over stdin/stdout. The child is killed when the client is dropped.

use std::collections::HashMap;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;

/// Core JSON-RPC 2.0 protocol handler, generic over reader/writer.
///
/// In production this is instantiated with `ChildStdin`/`BufReader<ChildStdout>`.
/// In tests it can use any `AsyncWrite`/`AsyncBufRead` implementation.
pub(crate) struct McpClientInner<W: tokio::io::AsyncWrite + Unpin, R: tokio::io::AsyncBufRead + Unpin> {
    writer: W,
    reader: R,
    next_id: u64,
}

impl<W: tokio::io::AsyncWrite + Unpin, R: tokio::io::AsyncBufRead + Unpin> McpClientInner<W, R> {
    pub(crate) async fn send_request(
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
        self.writer
            .write_all(line.as_bytes())
            .await
            .map_err(|e| format!("MCP write error: {e}"))?;
        self.writer
            .flush()
            .await
            .map_err(|e| format!("MCP flush error: {e}"))?;

        // Read response lines until we find one with a matching id.
        // Skip notifications (no "id" field) and unrelated messages.
        loop {
            let mut resp_line = String::new();
            let bytes = self
                .reader
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

    pub(crate) async fn send_notification(
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
        self.writer
            .write_all(line.as_bytes())
            .await
            .map_err(|e| format!("MCP write error: {e}"))?;
        self.writer
            .flush()
            .await
            .map_err(|e| format!("MCP flush error: {e}"))?;
        Ok(())
    }
}

/// Thread-safe MCP client. Wraps a child process with JSON-RPC communication.
pub struct McpClient {
    inner: Mutex<McpClientInner<ChildStdin, BufReader<ChildStdout>>>,
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
            writer: stdin,
            reader: BufReader::new(stdout),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Create an `McpClientInner` backed by in-memory buffers.
    /// `responses` is the newline-delimited JSON the "server" will send back.
    fn mock_inner(responses: &str) -> McpClientInner<Vec<u8>, Cursor<Vec<u8>>> {
        McpClientInner {
            writer: Vec::new(),
            reader: Cursor::new(responses.as_bytes().to_vec()),
            next_id: 1,
        }
    }

    // ── Request serialization ──────────────────────────────────────────

    #[tokio::test]
    async fn request_serialization_format() {
        // Server responds with a valid JSON-RPC result for id=1
        let response = r#"{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#.to_string() + "\n";
        let mut inner = mock_inner(&response);
        let result = inner.send_request("test/method", serde_json::json!({"key": "val"})).await;
        assert!(result.is_ok());

        // Check what was written to the "stdin"
        let written = String::from_utf8(inner.writer).unwrap();
        let req: serde_json::Value = serde_json::from_str(written.trim()).unwrap();
        assert_eq!(req["jsonrpc"], "2.0");
        assert_eq!(req["id"], 1);
        assert_eq!(req["method"], "test/method");
        assert_eq!(req["params"]["key"], "val");
    }

    #[tokio::test]
    async fn request_id_increments() {
        // Two responses for two sequential requests
        let responses = concat!(
            r#"{"jsonrpc":"2.0","id":1,"result":"first"}"#, "\n",
            r#"{"jsonrpc":"2.0","id":2,"result":"second"}"#, "\n",
        );
        let mut inner = mock_inner(responses);

        let r1 = inner.send_request("m1", serde_json::json!({})).await.unwrap();
        assert_eq!(r1, "first");

        let r2 = inner.send_request("m2", serde_json::json!({})).await.unwrap();
        assert_eq!(r2, "second");

        assert_eq!(inner.next_id, 3);

        // Verify both requests had sequential ids
        let written = String::from_utf8(inner.writer).unwrap();
        let lines: Vec<&str> = written.trim().split('\n').collect();
        assert_eq!(lines.len(), 2);
        let req1: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        let req2: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(req1["id"], 1);
        assert_eq!(req2["id"], 2);
    }

    // ── Response parsing ───────────────────────────────────────────────

    #[tokio::test]
    async fn response_success_returns_result_field() {
        let response = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#.to_string() + "\n";
        let mut inner = mock_inner(&response);
        let result = inner.send_request("tools/list", serde_json::json!({})).await.unwrap();
        assert_eq!(result, serde_json::json!({"tools": []}));
    }

    #[tokio::test]
    async fn response_error_returns_err() {
        let response = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32600,"message":"Invalid Request"}}"#.to_string() + "\n";
        let mut inner = mock_inner(&response);
        let result = inner.send_request("bad", serde_json::json!({})).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("MCP error response"), "got: {err}");
        assert!(err.contains("Invalid Request"), "got: {err}");
    }

    #[tokio::test]
    async fn response_null_result_returns_null() {
        let response = r#"{"jsonrpc":"2.0","id":1,"result":null}"#.to_string() + "\n";
        let mut inner = mock_inner(&response);
        let result = inner.send_request("test", serde_json::json!({})).await.unwrap();
        assert!(result.is_null());
    }

    // ── Notification vs request ────────────────────────────────────────

    #[tokio::test]
    async fn notification_has_no_id() {
        let mut inner = mock_inner("");
        let _ = inner.send_notification("notifications/initialized", serde_json::json!({})).await;

        let written = String::from_utf8(inner.writer).unwrap();
        let notif: serde_json::Value = serde_json::from_str(written.trim()).unwrap();
        assert_eq!(notif["jsonrpc"], "2.0");
        assert_eq!(notif["method"], "notifications/initialized");
        assert!(notif.get("id").is_none(), "notification must not have an id field");
    }

    #[tokio::test]
    async fn notification_does_not_increment_id() {
        let responses = r#"{"jsonrpc":"2.0","id":1,"result":"ok"}"#.to_string() + "\n";
        let mut inner = mock_inner(&responses);

        // Send notification — should not change next_id
        inner.send_notification("notify", serde_json::json!({})).await.unwrap();
        assert_eq!(inner.next_id, 1);

        // Send request — should use id=1
        let result = inner.send_request("req", serde_json::json!({})).await.unwrap();
        assert_eq!(result, "ok");
        assert_eq!(inner.next_id, 2);
    }

    // ── Skipping notifications in response stream ──────────────────────

    #[tokio::test]
    async fn skips_server_notifications_to_find_matching_response() {
        let responses = concat!(
            // Server sends a notification (no id) before the actual response
            r#"{"jsonrpc":"2.0","method":"log","params":{"message":"starting"}}"#, "\n",
            // Then sends an unrelated response with different id
            r#"{"jsonrpc":"2.0","id":99,"result":"wrong"}"#, "\n",
            // Then the actual matching response
            r#"{"jsonrpc":"2.0","id":1,"result":"correct"}"#, "\n",
        );
        let mut inner = mock_inner(responses);
        let result = inner.send_request("test", serde_json::json!({})).await.unwrap();
        assert_eq!(result, "correct");
    }

    #[tokio::test]
    async fn skips_empty_lines() {
        let responses = "\n\n".to_string() + r#"{"jsonrpc":"2.0","id":1,"result":42}"# + "\n";
        let mut inner = mock_inner(&responses);
        let result = inner.send_request("test", serde_json::json!({})).await.unwrap();
        assert_eq!(result, 42);
    }

    // ── Error handling ─────────────────────────────────────────────────

    #[tokio::test]
    async fn eof_returns_connection_closed_error() {
        // Empty response stream — server closed immediately
        let mut inner = mock_inner("");
        let result = inner.send_request("test", serde_json::json!({})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("closed connection"));
    }

    #[tokio::test]
    async fn malformed_json_returns_parse_error() {
        let responses = "this is not json\n";
        let mut inner = mock_inner(responses);
        let result = inner.send_request("test", serde_json::json!({})).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("parse error"), "got: {err}");
        assert!(err.contains("this is not json"), "raw line should be included, got: {err}");
    }

    #[tokio::test]
    async fn eof_after_notifications_returns_closed() {
        // Server sends a notification then closes — no matching response ever arrives
        let responses = r#"{"jsonrpc":"2.0","method":"log","params":{}}"#.to_string() + "\n";
        let mut inner = mock_inner(&responses);
        let result = inner.send_request("test", serde_json::json!({})).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("closed connection"));
    }

    // ── Multiple sequential requests ───────────────────────────────────

    #[tokio::test]
    async fn multiple_requests_work_sequentially() {
        let responses = concat!(
            r#"{"jsonrpc":"2.0","id":1,"result":"alpha"}"#, "\n",
            r#"{"jsonrpc":"2.0","id":2,"result":"beta"}"#, "\n",
            r#"{"jsonrpc":"2.0","id":3,"result":"gamma"}"#, "\n",
        );
        let mut inner = mock_inner(responses);

        assert_eq!(inner.send_request("a", serde_json::json!({})).await.unwrap(), "alpha");
        assert_eq!(inner.send_request("b", serde_json::json!({})).await.unwrap(), "beta");
        assert_eq!(inner.send_request("c", serde_json::json!({})).await.unwrap(), "gamma");
    }

    // ── Request with complex params ────────────────────────────────────

    #[tokio::test]
    async fn request_with_nested_params() {
        let response = r#"{"jsonrpc":"2.0","id":1,"result":null}"#.to_string() + "\n";
        let mut inner = mock_inner(&response);
        let params = serde_json::json!({
            "name": "fetch",
            "arguments": {"url": "https://example.com", "nested": [1, 2, 3]}
        });
        inner.send_request("tools/call", params.clone()).await.unwrap();

        let written = String::from_utf8(inner.writer).unwrap();
        let req: serde_json::Value = serde_json::from_str(written.trim()).unwrap();
        assert_eq!(req["params"], params);
    }
}
