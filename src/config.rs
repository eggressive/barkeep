use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub ai: AiConfig,
    #[serde(default)]
    pub mcp: McpConfig,
}

/// AI provider list. Ordered: the first provider that answers wins,
/// the rest are fallbacks. No brand owns the default - the order is
/// a user decision, not a compiled-in one.
#[derive(Debug, Clone, Deserialize)]
pub struct AiConfig {
    #[serde(default, rename = "provider")]
    pub providers: Vec<ProviderConfig>,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self::local_default()
    }
}

impl AiConfig {
    /// Zero-config default: local Ollama only.
    pub fn local_default() -> Self {
        Self {
            providers: vec![ProviderConfig {
                name: None,
                kind: ProviderKind::Ollama,
                model: "qwen3.5:0.8b".into(),
                url: Some("http://localhost:11434".into()),
                api_key: None,
                api_key_env: None,
            }],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ProviderKind {
    #[serde(rename = "ollama")]
    Ollama,
    #[serde(rename = "openai-compat")]
    OpenAiCompat,
    #[serde(rename = "anthropic")]
    Anthropic,
}

impl ProviderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderKind::Ollama => "ollama",
            ProviderKind::OpenAiCompat => "openai-compat",
            ProviderKind::Anthropic => "anthropic",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    /// Optional display name; defaults to kind + model.
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub kind: ProviderKind,
    pub model: String,
    /// Base URL. Defaults per kind: ollama -> localhost:11434.
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
    /// Environment variable holding the key (e.g. OPENROUTER_API_KEY).
    /// anthropic defaults to ANTHROPIC_API_KEY when unset.
    #[serde(default)]
    pub api_key_env: Option<String>,
}

impl ProviderConfig {
    /// Human-readable label for logs and error messages.
    pub fn label(&self) -> String {
        match &self.name {
            Some(n) => n.clone(),
            None => format!("{}:{}", self.kind.as_str(), self.model),
        }
    }

    pub fn base_url(&self) -> &str {
        self.url
            .as_deref()
            .unwrap_or(match self.kind {
                ProviderKind::Ollama => "http://localhost:11434",
                ProviderKind::OpenAiCompat | ProviderKind::Anthropic => "",
            })
            .trim_end_matches('/')
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct McpConfig {
    #[serde(default, rename = "server")]
    pub servers: Vec<ServerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

pub fn config_path() -> std::path::PathBuf {
    dirs::config_dir()
        .unwrap_or_default()
        .join("barkeep")
        .join("config.toml")
}

/// Parse config from a TOML string (pure, testable).
pub fn parse_toml(s: &str) -> Result<Config, String> {
    toml::from_str(s).map_err(|e| e.to_string())
}

/// Load config from the default path; missing file falls back to defaults.
pub fn load() -> Config {
    load_from(&config_path())
}

pub fn load_from(path: &std::path::Path) -> Config {
    match std::fs::read_to_string(path) {
        Ok(s) => parse_toml(&s).unwrap_or_else(|e| {
            tracing::warn!("invalid config at {}: {e}; using defaults", path.display());
            Config::default()
        }),
        Err(_) => Config::default(),
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ai: AiConfig::default(),
            mcp: McpConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config_gets_local_ollama_default() {
        let c = parse_toml("").unwrap();
        assert_eq!(c.ai.providers.len(), 1);
        assert_eq!(c.ai.providers[0].kind, ProviderKind::Ollama);
        assert!(c.mcp.servers.is_empty());
    }

    #[test]
    fn parses_ordered_provider_list() {
        let c = parse_toml(
            r#"
            [[ai.provider]]
            type = "ollama"
            model = "deepseek-v4.1-flash:cloud"

            [[ai.provider]]
            name = "local studio"
            type = "openai-compat"
            model = "qwen3.5:0.8b"
            url = "http://localhost:1234/v1"
            api_key_env = "STUDIO_KEY"

            [[ai.provider]]
            type = "anthropic"
            model = "claude-sonnet-4-5-20250929"
            api_key = "sk-ant-test"

            [[mcp.server]]
            name = "echo"
            command = "python3"
            args = ["srv.py"]
            "#,
        )
        .unwrap();
        assert_eq!(c.ai.providers.len(), 3);
        assert_eq!(c.ai.providers[0].kind, ProviderKind::Ollama);
        assert_eq!(c.ai.providers[0].model, "deepseek-v4.1-flash:cloud");
        assert_eq!(c.ai.providers[1].kind, ProviderKind::OpenAiCompat);
        assert_eq!(c.ai.providers[1].label(), "local studio".to_string());
        assert_eq!(c.ai.providers[2].kind, ProviderKind::Anthropic);
        assert_eq!(c.mcp.servers[0].command, "python3");
    }

    #[test]
    fn unknown_provider_type_is_rejected() {
        let e = parse_toml(
            r#"
            [[ai.provider]]
            type = "wetware"
            model = "x"
            "#,
        );
        assert!(e.is_err());
    }

    #[test]
    fn base_url_strips_trailing_slash() {
        let c = parse_toml(
            r#"
            [[ai.provider]]
            type = "ollama"
            model = "m"
            url = "http://localhost:11434/"
            "#,
        )
        .unwrap();
        assert_eq!(c.ai.providers[0].base_url(), "http://localhost:11434");
    }

    #[test]
    fn invalid_toml_is_error() {
        assert!(parse_toml("not [ toml").is_err());
    }
}
