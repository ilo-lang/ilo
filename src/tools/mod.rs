pub mod http_provider;
#[cfg(feature = "tools")]
pub mod mcp_client;
#[cfg(feature = "tools")]
pub mod mcp_provider;

use crate::interpreter::Value;
use std::future::Future;
use std::pin::Pin;

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)] // variants used when `tools` feature is enabled
pub enum ToolError {
    #[error("tool not configured: {0}")]
    NotConfigured(String),
    #[error("HTTP error calling '{0}': {1}")]
    Http(String, String),
    #[error("JSON error for tool '{0}': {1}")]
    Json(String, String),
    #[error("timeout calling tool '{0}'")]
    Timeout(String),
}

#[allow(dead_code)] // method used when `tools` feature is enabled
pub trait ToolProvider: Send + Sync {
    fn call(
        &self,
        name: &str,
        args: Vec<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + '_>>;
}

#[allow(dead_code)] // used in tests and when the tools feature is enabled
pub struct StubProvider;

impl ToolProvider for StubProvider {
    fn call(
        &self,
        _name: &str,
        _args: Vec<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + '_>> {
        Box::pin(async { Ok(Value::Ok(Box::new(Value::Nil))) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_error_display_not_configured() {
        let e = ToolError::NotConfigured("fetch".into());
        assert_eq!(e.to_string(), "tool not configured: fetch");
    }

    #[test]
    fn tool_error_display_http() {
        let e = ToolError::Http("fetch".into(), "connection refused".into());
        assert_eq!(e.to_string(), "HTTP error calling 'fetch': connection refused");
    }

    #[test]
    fn tool_error_display_json() {
        let e = ToolError::Json("fetch".into(), "invalid json".into());
        assert_eq!(e.to_string(), "JSON error for tool 'fetch': invalid json");
    }

    #[test]
    fn tool_error_display_timeout() {
        let e = ToolError::Timeout("fetch".into());
        assert_eq!(e.to_string(), "timeout calling tool 'fetch'");
    }

    #[test]
    fn stub_provider_returns_ok_nil() {
        let provider = StubProvider;
        let fut = provider.call("any", vec![]);
        let result = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(fut);
        let Ok(Value::Ok(inner)) = result else { panic!("expected Ok(Ok(_))") };
        assert_eq!(*inner, Value::Nil);
    }
}
