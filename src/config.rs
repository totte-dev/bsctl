use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default, rename = "current-context")]
    pub current_context: Option<String>,
    #[serde(default)]
    pub contexts: BTreeMap<String, ContextConfig>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ContextConfig {
    #[serde(rename = "base-url")]
    pub base_url: String,
    #[serde(default)]
    pub token: Option<String>,
}

impl Config {
    pub fn config_dir() -> PathBuf {
        dirs::config_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
            .unwrap_or_else(|| PathBuf::from(".config"))
            .join("bsctl")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.yaml")
    }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let mut config: Self = serde_yaml_neo::from_str(&content)
            .with_context(|| format!("Failed to parse {}", path.display()))?;

        // Resolve environment variable references in tokens
        for ctx in config.contexts.values_mut() {
            if let Some(token) = &ctx.token {
                ctx.token = Some(resolve_env(token));
            }
        }
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir();
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create {}", dir.display()))?;
        let path = Self::config_path();
        let content = serde_yaml_neo::to_string(self)?;
        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write {}", path.display()))?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn current(&self) -> Option<&ContextConfig> {
        self.current_context
            .as_ref()
            .and_then(|name| self.contexts.get(name))
    }
}

/// Resolve `${ENV_VAR}` references in a string
fn resolve_env(s: &str) -> String {
    if let Some(var) = s.strip_prefix("${").and_then(|s| s.strip_suffix('}')) {
        std::env::var(var).unwrap_or_else(|_| s.to_string())
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_config() {
        let yaml = r#"
current-context: dev
contexts:
  dev:
    base-url: http://localhost:7007
  prod:
    base-url: https://backstage.example.com
    token: my-static-token
"#;
        let config: Config = serde_yaml_neo::from_str(yaml).unwrap();
        assert_eq!(config.current_context.as_deref(), Some("dev"));
        assert_eq!(config.contexts.len(), 2);
        assert_eq!(config.contexts["dev"].base_url, "http://localhost:7007");
        assert_eq!(
            config.contexts["prod"].token.as_deref(),
            Some("my-static-token")
        );

        let current = config.current();
        assert!(current.is_some());
        assert_eq!(current.unwrap().base_url, "http://localhost:7007");
    }

    #[test]
    fn empty_config() {
        let config: Config = serde_yaml_neo::from_str("").unwrap();
        assert!(config.current_context.is_none());
        assert!(config.contexts.is_empty());
        assert!(config.current().is_none());
    }

    #[test]
    fn resolve_env_var() {
        // SAFETY: This test runs single-threaded via cargo test
        unsafe { std::env::set_var("BSCTL_TEST_TOKEN", "secret123") };
        assert_eq!(resolve_env("${BSCTL_TEST_TOKEN}"), "secret123");
        assert_eq!(resolve_env("plain-value"), "plain-value");
        assert_eq!(
            resolve_env("${NONEXISTENT_VAR_12345}"),
            "${NONEXISTENT_VAR_12345}"
        );
        unsafe { std::env::remove_var("BSCTL_TEST_TOKEN") };
    }
}
