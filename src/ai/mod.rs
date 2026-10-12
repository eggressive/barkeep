mod claude;
mod ollama;

pub use claude::{anthropic_key, ANTHROPIC_API_KEY_ENV};

use crate::config::AiConfig;
use futures_util::SinkExt;

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("no AI backend available (set ANTHROPIC_API_KEY or start Ollama)")]
    NoBackend,
    #[error("http error: {0}")]
    Http(String),
}

/// Events emitted while an answer streams in.
pub enum StreamEvent {
    Chunk(String),
    Done,
    Failed(String),
}

/// Ask the configured backend: Claude first (if a key exists), Ollama as
/// fallback. Streams chunks through an iced stream channel.
pub fn stream_answer(
    cfg: AiConfig,
    prompt: String,
) -> impl futures_util::Stream<Item = StreamEvent> {
    iced::stream::channel(
        16,
        async move |mut sender: iced::futures::channel::mpsc::Sender<StreamEvent>| {
            let key = anthropic_key(&cfg);
            if let Some(key) = key {
                match claude::stream(&key, &cfg.model, &prompt, &mut sender).await {
                    Ok(()) => {
                        let _ = sender.send(StreamEvent::Done).await;
                        return;
                    }
                    Err(e) => tracing::warn!("claude failed, falling back to ollama: {e}"),
                }
            }

            match ollama::stream(&cfg, &prompt, &mut sender).await {
                Ok(()) => {
                    let _ = sender.send(StreamEvent::Done).await;
                }
                Err(e) => {
                    let _ = sender.send(StreamEvent::Failed(e.to_string())).await;
                }
            }
        },
    )
}
