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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ToolProvider;

    #[test]
    fn from_file_missing_file() {
        let result = ToolsConfig::from_file("/nonexistent/path/config.json");
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("failed to read tools config"), "got: {msg}");
    }

    #[test]
    fn from_file_invalid_json() {
        let mut path = std::env::temp_dir();
        path.push("ilo_test_http_provider_invalid.json");
        std::fs::write(&path, "not valid json").unwrap();

        let result = ToolsConfig::from_file(path.to_str().unwrap());
        std::fs::remove_file(&path).ok();

        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("failed to parse tools config"), "got: {msg}");
    }

    #[test]
    fn from_file_valid_json() {
        let mut path = std::env::temp_dir();
        path.push("ilo_test_http_provider_valid.json");
        std::fs::write(
            &path,
            r#"{"tools":{"ping":{"url":"http://example.com"}}}"#,
        )
        .unwrap();

        let result = ToolsConfig::from_file(path.to_str().unwrap());
        std::fs::remove_file(&path).ok();

        assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
        let config = result.unwrap();
        assert!(config.tools.contains_key("ping"));
    }

    #[test]
    fn http_provider_new_constructs() {
        let config = ToolsConfig {
            tools: HashMap::new(),
        };
        let _provider = HttpProvider::new(config);
    }

    #[tokio::test]
    async fn call_without_tools_feature_returns_ok_nil() {
        // Without the `tools` feature the call stub returns Ok(Nil)
        let provider = HttpProvider::new(ToolsConfig {
            tools: HashMap::new(),
        });
        let result = provider.call("any_tool", vec![]).await;
        assert!(result.is_ok(), "expected Ok, got: {result:?}");
        assert_eq!(result.unwrap(), Value::Ok(Box::new(Value::Nil)));
    }

    #[tokio::test]
    async fn call_ignores_args_without_tools_feature() {
        let provider = HttpProvider::new(ToolsConfig {
            tools: HashMap::new(),
        });
        let args = vec![Value::Text("ignored".into()), Value::Number(42.0)];
        let result = provider.call("tool", args).await;
        assert_eq!(result.unwrap(), Value::Ok(Box::new(Value::Nil)));
    }
}
