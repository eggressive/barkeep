//! Minimal MCP types: JSON-RPC 2.0 over stdio (newline-delimited).

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTOCOL_VERSION: &str = "2025-06-18";
pub const JSONRPC: &str = "2.0";

#[derive(Debug, Serialize)]
pub struct RpcRequest {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct RpcNotification {
    pub jsonrpc: &'static str,
    pub method: &'static str,
}

/// Tolerant response: id may be absent (notifications), result/error either/or.
#[derive(Debug, Deserialize)]
pub struct RpcResponse {
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

/// A tool advertised by a server.
#[derive(Debug, Clone)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

impl ToolDef {
    pub fn from_value(v: &Value) -> Option<Self> {
        Some(Self {
            name: v["name"].as_str()?.to_string(),
            description: v["description"].as_str().unwrap_or_default().to_string(),
            input_schema: v
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
        })
    }
}

pub fn initialize_params(client_name: &str, client_version: &str) -> Value {
    serde_json::json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": {},
        "clientInfo": { "name": client_name, "version": client_version }
    })
}
