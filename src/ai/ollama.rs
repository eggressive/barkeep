use super::{AiError, ChunkSink};
use crate::config::ProviderConfig;
use futures_util::StreamExt;

/// Stream a completion from an Ollama server (native NDJSON /api/generate).
/// Handles thinking-capable models whose tokens arrive in `thinking`.
pub async fn stream(
    p: &ProviderConfig,
    prompt: &str,
    sink: &mut ChunkSink<'_>,
) -> Result<(), AiError> {
    let url = format!("{}/api/generate", p.base_url());
    let body = serde_json::json!({
        "model": p.model,
        "prompt": prompt,
        "stream": true,
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| AiError::Http(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(AiError::Http(format!("ollama returned {}", resp.status())));
    }

    let mut lines = String::new();
    let mut thinking = String::new();
    let mut saw_response = false;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| AiError::Http(e.to_string()))?;
        lines.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(pos) = lines.find('\n') {
            let line = lines[..pos].trim().to_string();
            lines.drain(..=pos);
            if line.is_empty() {
                continue;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if let Some(text) = v["response"].as_str() {
                if !text.is_empty() {
                    saw_response = true;
                    sink.chunk(text.to_string()).await;
                }
            }
            if let Some(t) = v["thinking"].as_str() {
                thinking.push_str(t);
            }
            if v["done"].as_bool() == Some(true) {
                // Thinking-capable models may emit only `thinking` tokens.
                if !saw_response && !thinking.is_empty() {
                    sink.chunk(thinking).await;
                }
                return Ok(());
            }
        }
    }
    Ok(())
}
