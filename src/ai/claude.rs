use super::{AiError, ChunkSink};
use futures_util::StreamExt;
use reqwest_eventsource::{Event, EventSource};

pub const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";

/// Stream a completion from the Anthropic Messages API via SSE.
pub async fn stream(
    api_key: &str,
    model: &str,
    prompt: &str,
    sink: &mut ChunkSink<'_>,
) -> Result<(), AiError> {
    let body = serde_json::json!({
        "model": model,
        "max_tokens": 1024,
        "stream": true,
        "messages": [{"role": "user", "content": prompt}]
    });

    let client = reqwest::Client::new();
    let request = client
        .post(API_URL)
        .header("content-type", "application/json")
        .header("x-api-key", api_key)
        .header("anthropic-version", API_VERSION)
        .json(&body);

    let mut es = EventSource::new(request).map_err(|e| AiError::Http(e.to_string()))?;

    while let Some(event) = es.next().await {
        match event {
            Ok(Event::Open) => {}
            Ok(Event::Message(msg)) => match msg.event.as_str() {
                "content_block_delta" => {
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(&msg.data) {
                        if let Some(text) = data["delta"]["text"].as_str() {
                            sink.chunk(text.to_string()).await;
                        }
                    }
                }
                "message_stop" => {
                    es.close();
                    return Ok(());
                }
                "error" => {
                    es.close();
                    let detail = serde_json::from_str::<serde_json::Value>(&msg.data)
                        .ok()
                        .and_then(|v| v["error"]["message"].as_str().map(String::from))
                        .unwrap_or_else(|| msg.data.clone());
                    return Err(AiError::Http(detail));
                }
                _ => {}
            },
            Err(e) => {
                es.close();
                return Err(AiError::Http(e.to_string()));
            }
        }
    }
    Ok(())
}
