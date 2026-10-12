pub mod client;
pub mod protocol;

pub use client::McpClient;
pub use protocol::ToolDef;

use crate::config::{McpConfig, ServerConfig};
use serde_json::Value;

/// One connected server with its tools.
pub struct ServerState {
    pub name: String,
    pub client: Option<McpClient>,
    pub tools: Vec<ToolDef>,
    pub error: Option<String>,
}

/// All configured MCP servers, best-effort connected.
pub struct Registry {
    pub servers: Vec<ServerState>,
}

impl Registry {
    /// Connect every configured server; failures are recorded per server.
    pub async fn connect(cfg: &McpConfig) -> Self {
        let mut servers = Vec::new();
        for sc in &cfg.servers {
            servers.push(connect_one(sc).await);
        }
        Self { servers }
    }

    /// (connected, failed) server counts.
    pub fn stats(&self) -> (usize, usize) {
        let ok = self.servers.iter().filter(|s| s.client.is_some()).count();
        (ok, self.servers.len() - ok)
    }

    /// Flat list of (server, tool) for display and lookup.
    pub fn tools(&self) -> Vec<(&str, &ToolDef)> {
        self.servers
            .iter()
            .flat_map(|s| s.tools.iter().map(move |t| (s.name.as_str(), t)))
            .collect()
    }

    pub async fn call(&mut self, server: &str, tool: &str, args: &Value) -> Result<Value, String> {
        let state = self
            .servers
            .iter_mut()
            .find(|s| s.name == server)
            .ok_or_else(|| format!("no server named '{server}'"))?;
        let client = state
            .client
            .as_mut()
            .ok_or_else(|| format!("server '{server}' not connected"))?;
        client
            .call_tool(tool, args.clone())
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn shutdown_all(self) {
        for s in self.servers {
            if let Some(c) = s.client {
                c.shutdown().await;
            }
        }
    }
}

async fn connect_one(sc: &ServerConfig) -> ServerState {
    let mut state = ServerState {
        name: sc.name.clone(),
        client: None,
        tools: Vec::new(),
        error: None,
    };
    match McpClient::spawn(&sc.name, &sc.command, &sc.args, &sc.env).await {
        Ok(mut client) => {
            let init = client.initialize().await;
            match init {
                Ok(()) => match client.list_tools().await {
                    Ok(tools) => {
                        tracing::info!("mcp {}: connected, {} tools", sc.name, tools.len());
                        state.tools = tools;
                        state.client = Some(client);
                    }
                    Err(e) => {
                        tracing::warn!("mcp {}: tools/list failed: {e}", sc.name);
                        state.error = Some(e.to_string());
                    }
                },
                Err(e) => {
                    tracing::warn!("mcp {}: initialize failed: {e}", sc.name);
                    state.error = Some(e.to_string());
                }
            }
        }
        Err(e) => {
            tracing::warn!("mcp {}: spawn failed: {e}", sc.name);
            state.error = Some(e.to_string());
        }
    }
    state
}
