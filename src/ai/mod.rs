mod claude;
mod ollama;
mod openai;

pub use claude::API_URL as ANTHROPIC_API_URL;

use crate::config::{ProviderConfig, ProviderKind};
use futures_util::SinkExt;

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("no API key configured for provider")]
    NoApiKey,
    #[error("http error: {0}")]
    Http(String),
}

/// Events emitted while an answer streams in.
pub enum StreamEvent {
    Chunk(String),
    Done,
    Failed(String),
}

/// Wraps the event sender; tracks whether any chunk made it out, so the
/// orchestrator never retries a provider that already answered partially.
pub struct ChunkSink<'a> {
    sender: &'a mut iced::futures::channel::mpsc::Sender<StreamEvent>,
    pub sent: bool,
}

impl<'a> ChunkSink<'a> {
    pub fn new(sender: &'a mut iced::futures::channel::mpsc::Sender<StreamEvent>) -> Self {
        Self {
            sender,
            sent: false,
        }
    }

    pub async fn chunk(&mut self, text: String) {
        self.sent = true;
        let _ = self.sender.send(StreamEvent::Chunk(text)).await;
    }
}

/// Resolve a provider's API key: inline value first, then env var
/// (anthropic defaults to ANTHROPIC_API_KEY when no env name is given).
pub(crate) fn resolve_key(p: &ProviderConfig) -> Option<String> {
    if let Some(k) = &p.api_key {
        if !k.trim().is_empty() {
            return Some(k.clone());
        }
    }
    let env_name = match &p.api_key_env {
        Some(e) => e.as_str(),
        None => match p.kind {
            ProviderKind::Anthropic => "ANTHROPIC_API_KEY",
            _ => return None,
        },
    };
    match std::env::var(env_name) {
        Ok(v) if !v.trim().is_empty() => Some(v),
        _ => None,
    }
}

/// Try each configured provider in order; the first one that answers wins.
/// A provider error with no chunks sent falls through to the next one.
pub fn ask(
    providers: Vec<ProviderConfig>,
    prompt: String,
) -> impl futures_util::Stream<Item = StreamEvent> {
    iced::stream::channel(16, async move |mut sender| {
        if providers.is_empty() {
            let _ = sender
                .send(StreamEvent::Failed("no AI provider configured".into()))
                .await;
            return;
        }

        for p in &providers {
            let mut sink = ChunkSink::new(&mut sender);
            let attempt = match p.kind {
                ProviderKind::Ollama => ollama::stream(p, &prompt, &mut sink).await,
                ProviderKind::OpenAiCompat => openai::stream(p, &prompt, &mut sink).await,
                ProviderKind::Anthropic => match resolve_key(p) {
                    Some(key) => claude::stream(&key, &p.model, &prompt, &mut sink).await,
                    None => Err(AiError::NoApiKey),
                },
            };

            match attempt {
                Ok(()) => {
                    let _ = sender.send(StreamEvent::Done).await;
                    return;
                }
                Err(e) => {
                    if sink.sent {
                        // Already streamed part of an answer; retrying would garble it.
                        let _ = sender
                            .send(StreamEvent::Failed(format!(
                                "{} failed mid-answer: {e}",
                                p.label()
                            )))
                            .await;
                        return;
                    }
                    tracing::warn!("provider {} unavailable: {e}", p.label());
                }
            }
        }

        let _ = sender
            .send(StreamEvent::Failed(
                "no AI provider succeeded (check `[[ai.provider]]` in config)".into(),
            ))
            .await;
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderKind;
    use futures_util::StreamExt;

    fn provider(kind: ProviderKind, model: &str, url: &str) -> crate::config::ProviderConfig {
        crate::config::ProviderConfig {
            name: None,
            kind,
            model: model.into(),
            url: Some(url.into()),
            api_key: None,
            api_key_env: None,
        }
    }

    async fn drain(
        mut rx: iced::futures::channel::mpsc::Receiver<StreamEvent>,
    ) -> Vec<StreamEvent> {
        let mut out = Vec::new();
        while let Some(ev) = rx.next().await {
            out.push(ev);
        }
        out
    }

    fn chunks(events: &[StreamEvent]) -> String {
        events
            .iter()
            .filter_map(|e| match e {
                StreamEvent::Chunk(c) => Some(c.as_str()),
                _ => None,
            })
            .collect()
    }

    // Live tests: require a running Ollama on localhost:11434.
    // Run with: cargo test -- --ignored

    #[tokio::test]
    #[ignore = "requires local Ollama"]
    async fn ollama_native_local_model_streams() {
        let (tx, rx) = iced::futures::channel::mpsc::channel(16);
        let mut tx = tx;
        let mut sink = ChunkSink::new(&mut tx);
        let p = provider(
            ProviderKind::Ollama,
            "qwen3.5:0.8b",
            "http://localhost:11434",
        );
        ollama::stream(&p, "Say OK.", &mut sink)
            .await
            .expect("stream ok");
        drop(tx);
        let events = drain(rx).await;
        assert!(!chunks(&events).is_empty(), "expected at least one chunk");
    }

    #[tokio::test]
    #[ignore = "requires local Ollama with cloud models"]
    async fn ollama_native_cloud_model_streams() {
        let (tx, rx) = iced::futures::channel::mpsc::channel(16);
        let mut tx = tx;
        let mut sink = ChunkSink::new(&mut tx);
        let p = provider(
            ProviderKind::Ollama,
            "deepseek-v4.1-flash:cloud",
            "http://localhost:11434",
        );
        ollama::stream(&p, "Say OK.", &mut sink)
            .await
            .expect("stream ok");
        drop(tx);
        let events = drain(rx).await;
        assert!(!chunks(&events).is_empty(), "expected at least one chunk");
    }

    #[tokio::test]
    #[ignore = "requires local Ollama OpenAI-compat endpoint"]
    async fn openai_compat_against_ollama_streams() {
        let (tx, rx) = iced::futures::channel::mpsc::channel(16);
        let mut tx = tx;
        let mut sink = ChunkSink::new(&mut tx);
        let p = provider(
            ProviderKind::OpenAiCompat,
            "qwen3.5:0.8b",
            "http://localhost:11434",
        );
        openai::stream(&p, "Say OK.", &mut sink)
            .await
            .expect("stream ok");
        drop(tx);
        let events = drain(rx).await;
        assert!(!chunks(&events).is_empty(), "expected at least one chunk");
    }
}
