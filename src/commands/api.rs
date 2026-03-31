use anyhow::Result;
use clap::Subcommand;

use crate::client::BackstageClient;

#[derive(Subcommand)]
pub enum ApiCommand {
    /// Send a GET request
    Get {
        /// API path (e.g. /api/catalog/entities)
        path: String,

        /// Query parameters as key=value pairs
        #[arg(long, short, value_name = "KEY=VALUE")]
        query: Vec<String>,
    },
    /// Send a POST request
    Post {
        /// API path
        path: String,

        /// Request body as JSON string
        #[arg(long, short)]
        body: Option<String>,

        /// Body parameters as key=value pairs (alternative to --body)
        #[arg(long = "param", short = 'p', value_name = "KEY=VALUE")]
        params: Vec<String>,
    },
    /// Send a PUT request
    Put {
        /// API path
        path: String,

        /// Request body as JSON string
        #[arg(long, short)]
        body: Option<String>,

        /// Body parameters as key=value pairs
        #[arg(long = "param", short = 'p', value_name = "KEY=VALUE")]
        params: Vec<String>,
    },
    /// Send a DELETE request
    Delete {
        /// API path
        path: String,
    },
}

pub async fn run(client: &BackstageClient, command: ApiCommand) -> Result<()> {
    match command {
        ApiCommand::Get { path, query } => get(client, &path, query).await,
        ApiCommand::Post { path, body, params } => {
            send_with_body(client, reqwest::Method::POST, &path, body, params).await
        }
        ApiCommand::Put { path, body, params } => {
            send_with_body(client, reqwest::Method::PUT, &path, body, params).await
        }
        ApiCommand::Delete { path } => delete(client, &path).await,
    }
}

fn build_query(path: &str, query: Vec<String>) -> String {
    if query.is_empty() {
        return path.to_string();
    }
    let qs: Vec<String> = query
        .iter()
        .map(|q| {
            let (k, v) = q.split_once('=').unwrap_or((q, ""));
            format!("{}={}", urlencoding::encode(k), urlencoding::encode(v))
        })
        .collect();
    format!("{path}?{}", qs.join("&"))
}

fn build_json_body(body: Option<String>, params: Vec<String>) -> Result<serde_json::Value> {
    if let Some(body_str) = body {
        Ok(serde_json::from_str(&body_str)?)
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
        Ok(serde_json::Value::Object(map))
    } else {
        Ok(serde_json::json!({}))
    }
}

async fn get(client: &BackstageClient, path: &str, query: Vec<String>) -> Result<()> {
    let full_path = build_query(path, query);
    let resp: serde_json::Value = client.get(&full_path).await?;
    println!("{}", serde_json::to_string_pretty(&resp)?);
    Ok(())
}

async fn send_with_body(
    client: &BackstageClient,
    method: reqwest::Method,
    path: &str,
    body: Option<String>,
    params: Vec<String>,
) -> Result<()> {
    let json_body = build_json_body(body, params)?;
    let resp: serde_json::Value = if method == reqwest::Method::PUT {
        client.put(path, &json_body).await?
    } else {
        client.post(path, &json_body).await?
    };
    println!("{}", serde_json::to_string_pretty(&resp)?);
    Ok(())
}

async fn delete(client: &BackstageClient, path: &str) -> Result<()> {
    let text = client.delete_raw(path).await?;
    if text.is_empty() {
        println!("OK");
    } else if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
        println!("{}", serde_json::to_string_pretty(&json)?);
    } else {
        println!("{text}");
    }
    Ok(())
}
