use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub ai: AiConfig,
    #[serde(default)]
    pub mcp: McpConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AiConfig {
    pub model: String,
    pub ollama_model: String,
    #[serde(default)]
    pub anthropic_key: Option<String>,
    #[serde(default = "default_ollama_url")]
    pub ollama_url: String,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-5-20250929".into(),
            ollama_model: "qwen3.5:0.8b".into(),
            anthropic_key: None,
            ollama_url: default_ollama_url(),
        }
    }
}

fn default_ollama_url() -> String {
    "http://localhost:11434".into()
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
    fn defaults_without_file() {
        let c = parse_toml("").unwrap();
        assert_eq!(c.ai.ollama_model, "qwen3.5:0.8b");
        assert!(c.mcp.servers.is_empty());
    }

    #[test]
    fn parses_servers_and_ai() {
        let c = parse_toml(
            r#"
            [ai]
            model = "claude-x"
            ollama_model = "llama9:latest"

            [[mcp.server]]
            name = "echo"
            command = "python3"
            args = ["srv.py"]
            "#,
        )
        .unwrap();
        assert_eq!(c.ai.model, "claude-x");
        assert_eq!(c.mcp.servers.len(), 1);
        assert_eq!(c.mcp.servers[0].command, "python3");
        assert_eq!(c.mcp.servers[0].args, vec!["srv.py"]);
    }

    #[test]
    fn invalid_toml_is_error() {
        assert!(parse_toml("not [ toml").is_err());
    }
}
