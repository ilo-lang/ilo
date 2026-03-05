use super::{ToolError, ToolProvider};
use crate::interpreter::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)] // fields used when `tools` feature is enabled
pub struct ToolEndpoint {
    pub url: String,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub timeout_secs: Option<f64>,
    #[serde(default)]
    pub retries: Option<u32>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ToolsConfig {
    #[allow(dead_code)] // used when `tools` feature is enabled
    pub tools: HashMap<String, ToolEndpoint>,
}

impl ToolsConfig {
    pub fn from_file(path: &str) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("failed to read tools config {path}: {e}"))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("failed to parse tools config {path}: {e}"))
    }
}

pub struct HttpProvider {
    #[allow(dead_code)] // used when `tools` feature is enabled
    config: ToolsConfig,
    #[cfg(feature = "tools")]
    client: reqwest::Client,
}

impl HttpProvider {
    pub fn new(config: ToolsConfig) -> Self {
        HttpProvider {
            config,
            #[cfg(feature = "tools")]
            client: reqwest::Client::new(),
        }
    }
}

impl ToolProvider for HttpProvider {
    fn call(
        &self,
        name: &str,
        args: Vec<Value>,
    ) -> Pin<Box<dyn Future<Output = Result<Value, ToolError>> + Send + '_>> {
        let name = name.to_string();
        Box::pin(async move {
            #[cfg(feature = "tools")]
            {
                let endpoint = self
                    .config
                    .tools
                    .get(&name)
                    .ok_or_else(|| ToolError::NotConfigured(name.clone()))?;

                let json_args: Vec<serde_json::Value> = args
                    .iter()
                    .map(|v| {
                        v.to_json()
                            .map_err(|e| ToolError::Json(name.clone(), e.to_string()))
                    })
                    .collect::<Result<_, _>>()?;

                let body = serde_json::json!({ "args": json_args });
                let timeout = std::time::Duration::from_secs_f64(
                    endpoint.timeout_secs.unwrap_or(30.0),
                );

                let method = endpoint.method.as_deref().unwrap_or("POST");
                let mut req = match method.to_uppercase().as_str() {
                    "GET" => self.client.get(&endpoint.url),
                    "POST" => self.client.post(&endpoint.url),
                    "PUT" => self.client.put(&endpoint.url),
                    "PATCH" => self.client.patch(&endpoint.url),
                    "DELETE" => self.client.delete(&endpoint.url),
                    _ => self.client.post(&endpoint.url),
                };

                for (k, v) in &endpoint.headers {
                    req = req.header(k, v);
                }

                let resp = req
                    .json(&body)
                    .timeout(timeout)
                    .send()
                    .await
                    .map_err(|e| {
                        if e.is_timeout() {
                            ToolError::Timeout(name.clone())
                        } else {
                            ToolError::Http(name.clone(), e.to_string())
                        }
                    })?;

                let status = resp.status();
                let json: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| ToolError::Json(name.clone(), e.to_string()))?;

                if status.is_success() {
                    let val = Value::from_json(&json, None)
                        .map_err(|e| ToolError::Json(name.clone(), e.to_string()))?;
                    Ok(Value::Ok(Box::new(val)))
                } else {
                    let err_msg = json.to_string();
                    Ok(Value::Err(Box::new(Value::Text(format!(
                        "HTTP {}: {}",
                        status, err_msg
                    )))))
                }
            }
            #[cfg(not(feature = "tools"))]
            {
                let _ = (name, args);
                Ok(Value::Ok(Box::new(Value::Nil)))
            }
        })
    }
}
