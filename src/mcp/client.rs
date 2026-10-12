use super::protocol::{self, RpcNotification, RpcRequest, RpcResponse, ToolDef};
use serde_json::Value;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};

pub const CLIENT_NAME: &str = "barkeep";
pub const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("server said: {0}")]
    Server(String),
    #[error("invalid response: {0}")]
    BadResponse(String),
    #[error("timed out waiting for response")]
    Timeout,
}

pub struct McpClient {
    pub server_name: String,
    child: Child,
    stdin: ChildStdin,
    reader: BufReader<tokio::process::ChildStdout>,
    next_id: u64,
}

impl McpClient {
    /// Spawn a server process with piped stdio.
    pub async fn spawn(
        server_name: &str,
        command: &str,
        args: &[String],
        env: &std::collections::HashMap<String, String>,
    ) -> Result<Self, McpError> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = cmd.spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::BadResponse("no stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::BadResponse("no stdout".into()))?;
        Ok(Self {
            server_name: server_name.to_string(),
            child,
            stdin,
            reader: BufReader::new(stdout),
            next_id: 0,
        })
    }

    async fn send_line(&mut self, line: &str) -> Result<(), McpError> {
        use tokio::io::AsyncWriteExt;
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        Ok(self.stdin.flush().await?)
    }

    /// Send a request and wait for the matching response line.
    async fn request(&mut self, method: &str, params: Option<Value>) -> Result<Value, McpError> {
        let id = self.next_id;
        self.next_id += 1;
        let req = RpcRequest {
            jsonrpc: protocol::JSONRPC,
            id,
            method: method.to_string(),
            params,
        };
        self.send_line(
            &serde_json::to_string(&req).map_err(|e| McpError::BadResponse(e.to_string()))?,
        )
        .await?;

        let fut = async {
            loop {
                let mut line = String::new();
                let n = self.reader.read_line(&mut line).await?;
                if n == 0 {
                    return Err(McpError::BadResponse("server closed stdout".into()));
                }
                let Ok(resp) = serde_json::from_str::<RpcResponse>(&line) else {
                    continue;
                };
                if resp.id != Some(id) {
                    continue;
                } // notifications from the server
                if let Some(err) = resp.error {
                    return Err(McpError::Server(err.message));
                }
                return Ok(resp.result.unwrap_or(Value::Null));
            }
        };
        tokio::time::timeout(REQUEST_TIMEOUT, fut)
            .await
            .map_err(|_| McpError::Timeout)?
    }

    /// MCP handshake: initialize, then the initialized notification.
    pub async fn initialize(&mut self) -> Result<(), McpError> {
        let result = self
            .request(
                "initialize",
                Some(protocol::initialize_params(CLIENT_NAME, CLIENT_VERSION)),
            )
            .await?;
        // Accept whatever version the server speaks; log it for debugging.
        if let Some(v) = result["protocolVersion"].as_str() {
            tracing::debug!("{}: server protocol {v}", self.server_name);
        }
        let note = RpcNotification {
            jsonrpc: protocol::JSONRPC,
            method: "notifications/initialized",
        };
        self.send_line(
            &serde_json::to_string(&note).map_err(|e| McpError::BadResponse(e.to_string()))?,
        )
        .await
    }

    pub async fn list_tools(&mut self) -> Result<Vec<ToolDef>, McpError> {
        let result = self.request("tools/list", None).await?;
        let tools = result["tools"]
            .as_array()
            .ok_or_else(|| McpError::BadResponse("tools/list: no tools array".into()))?;
        Ok(tools.iter().filter_map(ToolDef::from_value).collect())
    }

    pub async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<Value, McpError> {
        self.request(
            "tools/call",
            Some(serde_json::json!({ "name": name, "arguments": arguments })),
        )
        .await
    }

    pub async fn shutdown(mut self) {
        use tokio::io::AsyncWriteExt;
        let _ = self.stdin.shutdown().await;
        let _ = self.child.wait().await;
    }
}
