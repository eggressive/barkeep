use super::{AiError, ChunkSink};
use crate::config::ProviderConfig;
use futures_util::StreamExt;

/// One parsed SSE payload line from an OpenAI-compatible server.
#[derive(Debug, PartialEq, Eq)]
pub enum SseData {
    /// {"choices":[{"delta":{"content":"..."}}]}
    Content(String),
    /// {"choices":[{"delta":{"reasoning_content":"..."}}]}
    Reasoning(String),
    /// server-signalled end of stream
    Done,
    /// a data line that carries neither (keepalives, usage-only chunks)
    Other,
}

/// Classify a raw SSE line. `data: [DONE]` ends the stream; `data: {...}`
/// payloads yield content/reasoning deltas; everything else is ignored.
pub fn parse_sse_line(line: &str) -> Option<SseData> {
    let payload = line.strip_prefix("data:")?.trim();
    if payload == "[DONE]" {
        return Some(SseData::Done);
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) else {
        return Some(SseData::Other);
    };
    let delta = &v["choices"][0]["delta"];
    if let Some(c) = delta["content"].as_str() {
        if !c.is_empty() {
            return Some(SseData::Content(c.to_string()));
        }
    }
    // Reasoning tokens: DeepSeek-style APIs use `reasoning_content`,
    // Ollama's OpenAI-compat endpoint uses `reasoning`.
    for field in ["reasoning_content", "reasoning"] {
        if let Some(r) = delta[field].as_str() {
            if !r.is_empty() {
                return Some(SseData::Reasoning(r.to_string()));
            }
        }
    }
    Some(SseData::Other)
}

/// Endpoint for an OpenAI-compatible chat completions API. A base URL
/// already ending in `/v1` is respected; otherwise `/v1` is appended.
pub fn endpoint(base: &str) -> String {
    if base.ends_with("/v1") {
        format!("{base}/chat/completions")
    } else {
        format!("{base}/v1/chat/completions")
    }
}

/// Stream a completion from any OpenAI-compatible server
/// (LM Studio, vLLM, OpenRouter, OpenAI, Groq, Ollama's /v1, ...).
pub async fn stream(
    p: &ProviderConfig,
    prompt: &str,
    sink: &mut ChunkSink<'_>,
) -> Result<(), AiError> {
    let base = p.base_url();
    if base.is_empty() {
        return Err(AiError::Http("openai-compat provider needs a url".into()));
    }
    let url = endpoint(base);
    let body = serde_json::json!({
        "model": p.model,
        "stream": true,
        "messages": [{"role": "user", "content": prompt}],
    });

    let client = reqwest::Client::new();
    let mut req = client.post(&url).json(&body);
    if let Some(key) = super::resolve_key(p) {
        req = req.bearer_auth(key);
    }
    let resp = req.send().await.map_err(|e| AiError::Http(e.to_string()))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(AiError::Http(format!("{status}: {detail}")));
    }

    let mut lines = String::new();
    let mut reasoning = String::new();
    let mut saw_content = false;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| AiError::Http(e.to_string()))?;
        lines.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(pos) = lines.find('\n') {
            let line = lines[..pos].to_string();
            lines.drain(..=pos);
            match parse_sse_line(line.trim()) {
                Some(SseData::Content(c)) => {
                    saw_content = true;
                    sink.chunk(c).await;
                }
                Some(SseData::Reasoning(r)) => reasoning.push_str(&r),
                Some(SseData::Done) => {
                    // Reasoning-only models (DeepSeek R-style) may emit
                    // no content at all; show the reasoning instead.
                    if !saw_content && !reasoning.is_empty() {
                        sink.chunk(reasoning).await;
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_content_delta() {
        let line = "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}";
        assert_eq!(parse_sse_line(line), Some(SseData::Content("hi".into())));
    }

    #[test]
    fn parses_reasoning_delta() {
        let line = "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"think\"}}]}";
        assert_eq!(
            parse_sse_line(line),
            Some(SseData::Reasoning("think".into()))
        );
    }

    #[test]
    fn parses_ollama_reasoning_field() {
        // Ollama /v1 sends thinking in `reasoning` with empty `content`
        let line =
            "data: {\"choices\":[{\"delta\":{\"content\":\"\",\"reasoning\":\"Thinking\"}}]}";
        assert_eq!(
            parse_sse_line(line),
            Some(SseData::Reasoning("Thinking".into()))
        );
    }

    #[test]
    fn done_marker() {
        assert_eq!(parse_sse_line("data: [DONE]"), Some(SseData::Done));
    }

    #[test]
    fn ignores_comments_and_keepalives() {
        assert_eq!(parse_sse_line(": keepalive"), None);
        assert_eq!(parse_sse_line(""), None);
        assert_eq!(parse_sse_line("data: {}"), Some(SseData::Other));
        assert_eq!(parse_sse_line("data: not json"), Some(SseData::Other));
    }

    #[test]
    fn endpoint_appends_v1_only_when_missing() {
        assert_eq!(
            endpoint("http://x:1234"),
            "http://x:1234/v1/chat/completions"
        );
        assert_eq!(
            endpoint("http://x:1234/v1"),
            "http://x:1234/v1/chat/completions"
        );
    }
}
