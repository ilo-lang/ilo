pub mod http_provider;

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
