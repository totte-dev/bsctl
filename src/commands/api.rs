use anyhow::Result;
use clap::Subcommand;

use crate::client::BackstageClient;

#[derive(Subcommand)]
pub enum ApiCommand {
    /// Send a GET request to any Backstage API endpoint
    Get {
        /// API path (e.g. /api/catalog/entities)
        path: String,

        /// Query parameters as key=value pairs
        #[arg(long, short, value_name = "KEY=VALUE")]
        query: Vec<String>,
    },
    /// Send a POST request to any Backstage API endpoint
    Post {
        /// API path (e.g. /api/scaffolder/v2/tasks)
        path: String,

        /// Request body as JSON string
        #[arg(long, short)]
        body: Option<String>,

        /// Body parameters as key=value pairs (alternative to --body)
        #[arg(long = "param", short = 'p', value_name = "KEY=VALUE")]
        params: Vec<String>,
    },
}

pub async fn run(client: &BackstageClient, command: ApiCommand) -> Result<()> {
    match command {
        ApiCommand::Get { path, query } => get(client, &path, query).await,
        ApiCommand::Post { path, body, params } => post(client, &path, body, params).await,
    }
}

async fn get(client: &BackstageClient, path: &str, query: Vec<String>) -> Result<()> {
    let full_path = if query.is_empty() {
        path.to_string()
    } else {
        let qs: Vec<String> = query
            .iter()
            .map(|q| {
                let (k, v) = q.split_once('=').unwrap_or((q, ""));
                format!("{}={}", urlencoding::encode(k), urlencoding::encode(v))
            })
            .collect();
        format!("{path}?{}", qs.join("&"))
    };

    let resp: serde_json::Value = client.get(&full_path).await?;
    println!("{}", serde_json::to_string_pretty(&resp)?);
    Ok(())
}

async fn post(
    client: &BackstageClient,
    path: &str,
    body: Option<String>,
    params: Vec<String>,
) -> Result<()> {
    let json_body = if let Some(body_str) = body {
        serde_json::from_str(&body_str)?
    } else if !params.is_empty() {
        let mut map = serde_json::Map::new();
        for param in &params {
            let (key, value) = param.split_once('=').ok_or_else(|| {
                anyhow::anyhow!("Invalid parameter '{param}'. Expected format: key=value")
            })?;
            let json_value = serde_json::from_str(value)
                .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));
            map.insert(key.to_string(), json_value);
        }
        serde_json::Value::Object(map)
    } else {
        serde_json::json!({})
    };

    let resp: serde_json::Value = client.post(path, &json_body).await?;
    println!("{}", serde_json::to_string_pretty(&resp)?);
    Ok(())
}
