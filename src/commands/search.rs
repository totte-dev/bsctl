use anyhow::Result;
use clap::Subcommand;
use serde::Deserialize;

use crate::client::BackstageClient;
use crate::display::{self, Cell, Style};

#[derive(Subcommand)]
pub enum SearchCommand {
    /// Search the Backstage catalog
    Query {
        /// Search term
        term: String,

        /// Filter by entity type (e.g. software-catalog)
        #[arg(long, short = 't')]
        r#type: Option<String>,

        /// Max results
        #[arg(long, default_value = "25")]
        limit: u32,

        /// Output format
        #[arg(long, short, default_value = "table")]
        output: String,
    },
}

#[derive(Deserialize)]
struct SearchResponse {
    results: Vec<SearchResult>,
}

#[derive(Deserialize)]
struct SearchResult {
    #[serde(rename = "type")]
    result_type: String,
    document: SearchDocument,
}

#[derive(Deserialize)]
struct SearchDocument {
    #[serde(default)]
    title: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    location: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    namespace: Option<String>,
}

pub async fn run(client: &BackstageClient, command: SearchCommand) -> Result<()> {
    match command {
        SearchCommand::Query {
            term,
            r#type,
            limit,
            output,
        } => query(client, &term, r#type, limit, &output).await,
    }
}

async fn query(
    client: &BackstageClient,
    term: &str,
    r#type: Option<String>,
    limit: u32,
    output: &str,
) -> Result<()> {
    let mut params = vec![
        format!("term={}", urlencoding::encode(term)),
        format!("limit={limit}"),
    ];
    if let Some(t) = &r#type {
        params.push(format!("types[0]={}", urlencoding::encode(t)));
    }

    let path = format!("/api/search/query?{}", params.join("&"));
    let resp: SearchResponse = client.get(&path).await?;

    match output {
        "json" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!(
                    resp.results
                        .iter()
                        .map(|r| serde_json::json!({
                            "type": r.result_type,
                            "title": r.document.title,
                            "text": r.document.text,
                            "location": r.document.location,
                            "kind": r.document.kind,
                            "namespace": r.document.namespace,
                        }))
                        .collect::<Vec<_>>()
                ))?
            );
        }
        _ => {
            let rows: Vec<Vec<Cell>> = resp
                .results
                .iter()
                .map(|r| {
                    vec![
                        Cell::styled(
                            r.document.kind.as_deref().unwrap_or(&r.result_type),
                            Style::Dim,
                        ),
                        Cell::new(&r.document.title),
                        Cell::styled(&r.document.location, Style::Dim),
                    ]
                })
                .collect();
            display::table(&["Kind", "Title", "Location"], &rows);
        }
    }
    Ok(())
}
