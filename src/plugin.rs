use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::client::BackstageClient;

#[derive(Deserialize, Default)]
pub struct PluginConfig {
    #[serde(default)]
    pub plugins: BTreeMap<String, BTreeMap<String, CommandDef>>,
    /// Custom columns per entity type for `catalog list`
    #[serde(default)]
    pub columns: BTreeMap<String, Vec<ColumnDef>>,
}

#[derive(Deserialize, Clone)]
pub struct ColumnDef {
    /// Column header text
    pub header: String,
    /// Dot-separated path to extract value from entity JSON
    /// e.g. "metadata.annotations.tactna.io/client-account-id"
    pub path: String,
    /// Optional style: "env" applies environment coloring (dev=blue, preview=yellow, prod=green)
    #[serde(default)]
    pub style: Option<String>,
}

impl ColumnDef {
    /// Extract a value from a JSON entity using a dot-separated path.
    ///
    /// For paths like `metadata.annotations.tactna.io/client-account-id`,
    /// when a segment isn't found as a direct key, the remaining path is
    /// joined and tried as a single key (to handle annotation keys with dots).
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

    // Try exact segment match first
    if let Some(child) = value.get(segments[0]) {
        let result = resolve_path(child, &segments[1..]);
        if !result.is_null() {
            return result;
        }
    }

    // If exact match fails, try joining remaining segments as a single key
    // This handles annotation keys like "tactna.io/client-account-id"
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

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
pub struct ArgDef {
    pub name: String,
    /// 1-based positional index
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
    /// Maps to query parameter key
    #[serde(default)]
    pub query: Option<String>,
    /// Maps to JSON body key
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub required: Option<bool>,
    #[serde(default)]
    pub description: Option<String>,
}

impl PluginConfig {
    pub fn load() -> Result<Self> {
        // Try .bsctl/ directory first (split files), then single .bsctl.yaml
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

        // Load plugins from plugins.yaml
        let plugins_path = dir.join("plugins.yaml");
        if plugins_path.exists() {
            let content = std::fs::read_to_string(&plugins_path)
                .with_context(|| format!("Failed to read {}", plugins_path.display()))?;
            let partial: PluginsFile = serde_yaml_neo::from_str(&content)
                .with_context(|| format!("Failed to parse {}", plugins_path.display()))?;
            config.plugins = partial;
        }

        // Load columns from columns.yaml or columns/*.yaml
        let columns_dir = dir.join("columns");
        if columns_dir.is_dir() {
            // Load each file in columns/ as a type-specific column definition
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
            // Fallback: single columns.yaml
            let columns_path = dir.join("columns.yaml");
            if columns_path.exists() {
                let content = std::fs::read_to_string(&columns_path)
                    .with_context(|| format!("Failed to read {}", columns_path.display()))?;
                let partial: ColumnsFile = serde_yaml_neo::from_str(&content)
                    .with_context(|| format!("Failed to parse {}", columns_path.display()))?;
                config.columns = partial;
            }
        }

        Ok(config)
    }

    fn load_from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let config: Self = serde_yaml_neo::from_str(&content)
            .with_context(|| format!("Failed to parse {}", path.display()))?;
        Ok(config)
    }
}

/// plugins.yaml: top-level keys are plugin names directly
type PluginsFile = BTreeMap<String, BTreeMap<String, CommandDef>>;

/// columns.yaml: top-level keys are entity type names directly
type ColumnsFile = BTreeMap<String, Vec<ColumnDef>>;

pub async fn run(
    client: &BackstageClient,
    plugin_name: &str,
    subcommand: &str,
    positional_args: &[String],
    named_params: &[(String, String)],
    config: &PluginConfig,
) -> Result<()> {
    let commands = config
        .plugins
        .get(plugin_name)
        .ok_or_else(|| anyhow::anyhow!("Unknown plugin: {plugin_name}"))?;

    let cmd = commands.get(subcommand).ok_or_else(|| {
        let available: Vec<&String> = commands.keys().collect();
        anyhow::anyhow!(
            "Unknown command '{subcommand}' for plugin '{plugin_name}'. Available: {}",
            available
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;

    // Build path with positional arg substitution
    let mut path = cmd.path.clone();
    for arg_def in &cmd.args {
        let idx = arg_def.position - 1;
        let value = positional_args
            .get(idx)
            .ok_or_else(|| anyhow::anyhow!("Missing required argument: {}", arg_def.name))?;
        path = path.replace(&format!("{{{}}}", arg_def.name), value);
    }

    // Build query params and body from named params
    let mut query_parts = Vec::new();
    let mut body_map = serde_json::Map::new();

    for param_def in &cmd.params {
        let value = named_params
            .iter()
            .find(|(k, _)| k == &param_def.name)
            .map(|(_, v)| v);

        if value.is_none() && param_def.required.unwrap_or(false) {
            anyhow::bail!("Missing required parameter: --{}", param_def.name);
        }

        if let Some(val) = value {
            if let Some(query_key) = &param_def.query {
                query_parts.push(format!(
                    "{}={}",
                    urlencoding::encode(query_key),
                    urlencoding::encode(val)
                ));
            }
            if let Some(body_key) = &param_def.body {
                let json_val = serde_json::from_str(val)
                    .unwrap_or_else(|_| serde_json::Value::String(val.to_string()));
                body_map.insert(body_key.to_string(), json_val);
            }
        }
    }

    if !query_parts.is_empty() {
        let sep = if path.contains('?') { "&" } else { "?" };
        path = format!("{path}{sep}{}", query_parts.join("&"));
    }

    let resp: serde_json::Value = match cmd.method {
        Method::Get | Method::Delete => client.get(&path).await?,
        Method::Post | Method::Put => {
            let body = serde_json::Value::Object(body_map);
            client.post(&path, &body).await?
        }
    };

    println!("{}", serde_json::to_string_pretty(&resp)?);
    Ok(())
}

pub fn print_plugin_help(config: &PluginConfig) {
    if config.plugins.is_empty() {
        return;
    }
    println!("\nPlugin commands (from .bsctl.yaml):");
    for (plugin, commands) in &config.plugins {
        for (cmd_name, cmd_def) in commands {
            let desc = cmd_def.description.as_deref().unwrap_or("");
            println!("  {plugin} {cmd_name:<20} {desc}");
        }
    }
}

pub fn print_command_help(plugin_name: &str, subcommand: &str, cmd: &CommandDef) {
    let desc = cmd.description.as_deref().unwrap_or("No description");
    println!("{desc}\n");
    println!("Usage: bsctl {plugin_name} {subcommand}{}", {
        let mut parts = String::new();
        for arg in &cmd.args {
            if arg.required.unwrap_or(true) {
                parts.push_str(&format!(" <{}>", arg.name));
            } else {
                parts.push_str(&format!(" [{}]", arg.name));
            }
        }
        for param in &cmd.params {
            if param.required.unwrap_or(false) {
                parts.push_str(&format!(" --{} <VALUE>", param.name));
            } else {
                parts.push_str(&format!(" [--{} <VALUE>]", param.name));
            }
        }
        parts
    });
    println!("\nMethod: {:?} {}", cmd.method, cmd.path);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_config() {
        let yaml = "plugins: {}";
        let config: PluginConfig = serde_yaml_neo::from_str(yaml).unwrap();
        assert!(config.plugins.is_empty());
    }

    #[test]
    fn parse_defaults_when_empty() {
        let yaml = "";
        let config: PluginConfig = serde_yaml_neo::from_str(yaml).unwrap();
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

        let tf = &config.plugins["terraform"];
        assert_eq!(tf.len(), 2);
        assert_eq!(tf["prs"].path, "/api/terraform-ops/infra-prs");
        assert!(matches!(tf["prs"].method, Method::Get));
        assert_eq!(tf["prs"].description.as_deref(), Some("List PRs"));

        assert_eq!(tf["merge"].args.len(), 1);
        assert_eq!(tf["merge"].args[0].name, "number");
        assert_eq!(tf["merge"].args[0].position, 1);

        let costs = &config.plugins["costs"];
        assert_eq!(costs["get"].params.len(), 1);
        assert_eq!(costs["get"].params[0].query.as_deref(), Some("accountId"));
        assert_eq!(costs["get"].params[0].required, Some(true));
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
}
