use anyhow::Result;
use clap::Subcommand;
use serde::Deserialize;

use crate::client::BackstageClient;
use crate::display::{self, Cell, Style};
use crate::service;

#[derive(Subcommand)]
pub enum SearchCommand {
    /// Search the Backstage catalog
    Query {
        term: String,
        #[arg(long, short = 't')]
        r#type: Option<String>,
        #[arg(long, default_value = "25")]
        limit: u32,
        #[arg(long, short, default_value = "table")]
        output: String,
    },
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
    location: String,
    #[serde(default)]
    kind: Option<String>,
}

pub async fn run(client: &BackstageClient, command: SearchCommand) -> Result<()> {
    match command {
        SearchCommand::Query {
            term,
            r#type,
            limit,
            output,
        } => query(client, &term, r#type.as_deref(), limit, &output).await,
    }
}

async fn query(
    client: &BackstageClient,
    term: &str,
    r#type: Option<&str>,
    limit: u32,
    output: &str,
) -> Result<()> {
    let resp = service::search(client, term, r#type, limit).await?;

    match output {
        "json" => println!("{}", serde_json::to_string_pretty(&resp)?),
        _ => {
            let results: Vec<SearchResult> = resp
                .get("results")
                .cloned()
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_default();

            let rows: Vec<Vec<Cell>> = results
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
