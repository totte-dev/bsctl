use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Deserialize, Default, Clone)]
pub struct PluginConfig {
    #[serde(default)]
    pub plugins: BTreeMap<String, BTreeMap<String, CommandDef>>,
    #[serde(default)]
    pub columns: BTreeMap<String, Vec<ColumnDef>>,
    #[serde(skip)]
    pub column_ignores: Vec<String>,
}

#[derive(Deserialize, Clone)]
pub struct ColumnDef {
    pub header: String,
    pub path: String,
    #[serde(default)]
    pub style: Option<String>,
}

impl ColumnDef {
    pub fn extract(&self, entity: &serde_json::Value) -> String {
        let segments: Vec<&str> = self.path.split('.').collect();
        let result = resolve_path(entity, &segments);
        match result {
            serde_json::Value::String(s) => s,
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::Bool(b) => b.to_string(),
            serde_json::Value::Null => String::new(),
            other => other.to_string(),
        }
    }
}

fn resolve_path(value: &serde_json::Value, segments: &[&str]) -> serde_json::Value {
    if segments.is_empty() {
        return value.clone();
    }
    if let Some(child) = value.get(segments[0]) {
        let result = resolve_path(child, &segments[1..]);
        if !result.is_null() {
            return result;
        }
    }
    if segments.len() > 1 {
        let joined = segments.join(".");
        if let Some(child) = value.get(&joined) {
            return child.clone();
        }
    }
    serde_json::Value::Null
}

#[derive(Deserialize, Clone)]
pub struct CommandDef {
    pub method: Method,
    pub path: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub args: Vec<ArgDef>,
    #[serde(default)]
    pub params: Vec<ParamDef>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
}

impl std::fmt::Debug for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Method::Get => write!(f, "GET"),
            Method::Post => write!(f, "POST"),
            Method::Put => write!(f, "PUT"),
            Method::Delete => write!(f, "DELETE"),
        }
    }
}

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
pub struct ArgDef {
    pub name: String,
    pub position: usize,
    #[serde(default)]
    pub required: Option<bool>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
pub struct ParamDef {
    pub name: String,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub required: Option<bool>,
    #[serde(default)]
    pub description: Option<String>,
}

// -- Loading --

type PluginsFile = BTreeMap<String, BTreeMap<String, CommandDef>>;
type ColumnsFile = BTreeMap<String, Vec<ColumnDef>>;

impl PluginConfig {
    pub fn load() -> Result<Self> {
        let dir_candidates = [
            std::env::current_dir().ok().map(|p| p.join(".bsctl")),
            dirs::home_dir().map(|p| p.join(".bsctl")),
        ];
        for dir in dir_candidates.into_iter().flatten() {
            if dir.is_dir() {
                return Self::load_from_dir(&dir);
            }
        }
        let file_candidates = [
            std::env::current_dir().ok().map(|p| p.join(".bsctl.yaml")),
            std::env::current_dir().ok().map(|p| p.join(".bsctl.yml")),
            dirs::home_dir().map(|p| p.join(".bsctl.yaml")),
            dirs::home_dir().map(|p| p.join(".bsctl.yml")),
        ];
        for candidate in file_candidates.into_iter().flatten() {
            if candidate.exists() {
                return Self::load_from_file(&candidate);
            }
        }
        Ok(Self::default())
    }

    fn load_from_dir(dir: &Path) -> Result<Self> {
        let mut config = Self::default();

        let plugins_path = dir.join("plugins.yaml");
        if plugins_path.exists() {
            let content = std::fs::read_to_string(&plugins_path)
                .with_context(|| format!("Failed to read {}", plugins_path.display()))?;
            let partial: PluginsFile = serde_yaml_neo::from_str(&content)
                .with_context(|| format!("Failed to parse {}", plugins_path.display()))?;
            config.plugins = partial;
        }

        let columns_dir = dir.join("columns");
        if columns_dir.is_dir() {
            let mut entries: Vec<_> = std::fs::read_dir(&columns_dir)?
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.path()
                        .extension()
                        .is_some_and(|ext| ext == "yaml" || ext == "yml")
                })
                .collect();
            entries.sort_by_key(|e| e.path());
            for entry in entries {
                let path = entry.path();
                let type_name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
                let content = std::fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read {}", path.display()))?;
                let cols: Vec<ColumnDef> = serde_yaml_neo::from_str(&content)
                    .with_context(|| format!("Failed to parse {}", path.display()))?;
                config.columns.insert(type_name, cols);
            }
        } else {
            let columns_path = dir.join("columns.yaml");
            if columns_path.exists() {
                let content = std::fs::read_to_string(&columns_path)
                    .with_context(|| format!("Failed to read {}", columns_path.display()))?;
                let partial: ColumnsFile = serde_yaml_neo::from_str(&content)
                    .with_context(|| format!("Failed to parse {}", columns_path.display()))?;
                config.columns = partial;
            }
        }

        let ignore_path = dir.join("columns.ignore");
        if ignore_path.exists() {
            config.column_ignores = load_ignore_patterns(&ignore_path)?;
        }
        config.apply_column_ignores();

        Ok(config)
    }

    fn load_from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let config: Self = serde_yaml_neo::from_str(&content)
            .with_context(|| format!("Failed to parse {}", path.display()))?;
        Ok(config)
    }

    fn apply_column_ignores(&mut self) {
        if self.column_ignores.is_empty() {
            return;
        }
        for columns in self.columns.values_mut() {
            columns.retain(|col| !is_path_ignored(&col.path, &self.column_ignores));
        }
    }
}

fn load_ignore_patterns(path: &Path) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    Ok(content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect())
}

pub fn is_path_ignored(path: &str, patterns: &[String]) -> bool {
    let key = path.strip_prefix("metadata.annotations.").unwrap_or(path);
    patterns.iter().any(|pattern| {
        if let Some(suffix) = pattern.strip_prefix('*') {
            key.ends_with(suffix)
        } else if let Some(prefix) = pattern.strip_suffix('*') {
            key.starts_with(prefix)
        } else {
            key == pattern
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_config() {
        let config: PluginConfig = serde_yaml_neo::from_str("plugins: {}").unwrap();
        assert!(config.plugins.is_empty());
    }

    #[test]
    fn parse_defaults_when_empty() {
        let config: PluginConfig = serde_yaml_neo::from_str("").unwrap();
        assert!(config.plugins.is_empty());
    }

    #[test]
    fn parse_plugin_config() {
        let yaml = r#"
plugins:
  terraform:
    prs:
      method: GET
      path: /api/terraform-ops/infra-prs
      description: List PRs
    merge:
      method: POST
      path: /api/terraform-ops/pr/{number}/merge
      args:
        - name: number
          position: 1
  costs:
    get:
      method: GET
      path: /api/aws-costs/costs
      params:
        - name: account-id
          query: accountId
          required: true
"#;
        let config: PluginConfig = serde_yaml_neo::from_str(yaml).unwrap();
        assert_eq!(config.plugins.len(), 2);
        assert_eq!(
            config.plugins["terraform"]["prs"].path,
            "/api/terraform-ops/infra-prs"
        );
        assert!(matches!(
            config.plugins["terraform"]["prs"].method,
            Method::Get
        ));
    }

    #[test]
    fn path_substitution() {
        let yaml = r#"
plugins:
  test:
    detail:
      method: GET
      path: /api/items/{id}/sub/{sub_id}
      args:
        - name: id
          position: 1
        - name: sub_id
          position: 2
"#;
        let config: PluginConfig = serde_yaml_neo::from_str(yaml).unwrap();
        let cmd = &config.plugins["test"]["detail"];
        let mut path = cmd.path.clone();
        let positional = vec!["42".to_string(), "abc".to_string()];
        for arg_def in &cmd.args {
            let value = &positional[arg_def.position - 1];
            path = path.replace(&format!("{{{}}}", arg_def.name), value);
        }
        assert_eq!(path, "/api/items/42/sub/abc");
    }

    #[test]
    fn ignore_suffix_pattern() {
        let patterns = vec!["*/terraform-path".into(), "*/suffix".into()];
        assert!(is_path_ignored(
            "metadata.annotations.tactna.io/terraform-path",
            &patterns
        ));
        assert!(!is_path_ignored(
            "metadata.annotations.tactna.io/customer",
            &patterns
        ));
    }

    #[test]
    fn ignore_prefix_pattern() {
        let patterns = vec!["backstage.io/*".into()];
        assert!(is_path_ignored(
            "backstage.io/managed-by-location",
            &patterns
        ));
        assert!(!is_path_ignored("tactna.io/customer", &patterns));
    }

    #[test]
    fn ignore_exact_pattern() {
        let patterns = vec!["tactna.io/internal-only".into()];
        assert!(is_path_ignored("tactna.io/internal-only", &patterns));
        assert!(!is_path_ignored("tactna.io/customer", &patterns));
    }
}
