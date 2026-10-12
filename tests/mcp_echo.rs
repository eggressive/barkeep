use barkeep::mcp::{McpClient, ToolDef};
use serde_json::json;
use std::collections::HashMap;

async fn spawn_echo() -> McpClient {
    let manifest =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/echo_mcp_server.py");
    McpClient::spawn(
        "echo-fixture",
        "python3",
        &[manifest.to_string_lossy().into_owned()],
        &HashMap::new(),
    )
    .await
    .expect("spawn fixture server (is python3 available?)")
}

#[tokio::test]
async fn handshake_and_list_tools() {
    let mut client = spawn_echo().await;
    client.initialize().await.expect("initialize");
    let tools: Vec<ToolDef> = client.list_tools().await.expect("tools/list");

    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "echo");
    assert!(!tools[0].description.is_empty());
}

#[tokio::test]
async fn call_tool_returns_text() {
    let mut client = spawn_echo().await;
    client.initialize().await.expect("initialize");

    let result = client
        .call_tool("echo", json!({ "text": "hello barkeep" }))
        .await
        .expect("tools/call");
    let text = result["content"][0]["text"].as_str().expect("text content");
    assert!(text.contains("hello barkeep"), "got: {text}");
}

#[tokio::test]
async fn unknown_tool_is_server_error() {
    let mut client = spawn_echo().await;
    client.initialize().await.expect("initialize");

    let err = client
        .call_tool("nope", json!({}))
        .await
        .expect_err("must fail");
    assert!(err.to_string().contains("Unknown tool"), "got: {err}");
}
