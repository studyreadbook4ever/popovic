use reqwest::Client;
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct McpEndpoint {
    pub name: String,
    pub url: String,
    pub bearer_token: String,
}

#[derive(Debug, Clone)]
pub struct McpClient {
    http: Client,
}

impl McpClient {
    pub fn new() -> Self {
        Self {
            http: Client::new(),
        }
    }

    pub async fn list_tools(&self, endpoint: &McpEndpoint) -> Result<Value, String> {
        self.rpc(endpoint, "tools/list", json!({})).await
    }

    #[allow(dead_code)]
    pub async fn call_tool(
        &self,
        endpoint: &McpEndpoint,
        tool_name: &str,
        arguments: Value,
    ) -> Result<Value, String> {
        self.rpc(
            endpoint,
            "tools/call",
            json!({
                "name": tool_name,
                "arguments": arguments
            }),
        )
        .await
    }

    async fn rpc(
        &self,
        endpoint: &McpEndpoint,
        method: &str,
        params: Value,
    ) -> Result<Value, String> {
        if endpoint.url.trim().is_empty() {
            return Err(format!("{} MCP URL is not configured", endpoint.name));
        }
        let request = json!({
            "jsonrpc": "2.0",
            "id": Uuid::new_v4().to_string(),
            "method": method,
            "params": params
        });
        let mut builder = self
            .http
            .post(&endpoint.url)
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream");
        if !endpoint.bearer_token.trim().is_empty() {
            builder = builder.bearer_auth(&endpoint.bearer_token);
        }
        let response = builder
            .json(&request)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        let status = response.status();
        let text = response.text().await.map_err(|error| error.to_string())?;
        if !status.is_success() {
            return Err(format!("MCP request failed with {status}: {text}"));
        }
        serde_json::from_str(&text).map_err(|error| format!("MCP response was not JSON: {error}"))
    }
}
