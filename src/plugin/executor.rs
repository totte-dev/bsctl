use anyhow::Result;

use super::config::{CommandDef, Method, PluginConfig};
use crate::client::BackstageClient;

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

    let path = build_path(cmd, positional_args, named_params)?;
    let body = build_body(cmd, named_params);

    match cmd.method {
        Method::Delete => {
            let text = client.delete_raw(&path).await?;
            if text.is_empty() {
                println!("OK");
            } else if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                println!("{}", serde_json::to_string_pretty(&json)?);
            } else {
                println!("{text}");
            }
        }
        _ => {
            let resp: serde_json::Value = match cmd.method {
                Method::Get => client.get(&path).await?,
                Method::Post => client.post(&path, &body).await?,
                Method::Put => client.put(&path, &body).await?,
                Method::Delete => unreachable!(),
            };
            println!("{}", serde_json::to_string_pretty(&resp)?);
        }
    }
    Ok(())
}

pub fn build_path(
    cmd: &CommandDef,
    positional_args: &[String],
    named_params: &[(String, String)],
) -> Result<String> {
    let mut path = cmd.path.clone();
    for arg_def in &cmd.args {
        let idx = arg_def.position - 1;
        let value = positional_args
            .get(idx)
            .ok_or_else(|| anyhow::anyhow!("Missing required argument: {}", arg_def.name))?;
        let encoded = urlencoding::encode(value);
        path = path.replace(&format!("{{{}}}", arg_def.name), &encoded);
    }

    let mut query_parts = Vec::new();
    for param_def in &cmd.params {
        let value = named_params
            .iter()
            .find(|(k, _)| k == &param_def.name)
            .map(|(_, v)| v);

        if value.is_none() && param_def.required.unwrap_or(false) {
            anyhow::bail!("Missing required parameter: --{}", param_def.name);
        }

        if let Some(val) = value
            && let Some(query_key) = &param_def.query
        {
            query_parts.push(format!(
                "{}={}",
                urlencoding::encode(query_key),
                urlencoding::encode(val)
            ));
        }
    }

    if !query_parts.is_empty() {
        let sep = if path.contains('?') { "&" } else { "?" };
        path = format!("{path}{sep}{}", query_parts.join("&"));
    }

    Ok(path)
}

pub fn build_body(cmd: &CommandDef, named_params: &[(String, String)]) -> serde_json::Value {
    let mut body_map = serde_json::Map::new();
    for param_def in &cmd.params {
        if let Some(body_key) = &param_def.body
            && let Some(val) = named_params
                .iter()
                .find(|(k, _)| k == &param_def.name)
                .map(|(_, v)| v)
        {
            let json_val = serde_json::from_str(val)
                .unwrap_or_else(|_| serde_json::Value::String(val.to_string()));
            body_map.insert(body_key.to_string(), json_val);
        }
    }
    serde_json::Value::Object(body_map)
}
