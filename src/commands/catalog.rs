use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use serde::Deserialize;

use crate::client::BackstageClient;
use crate::display::{self, Cell, Style};
use crate::service;

#[derive(Subcommand)]
pub enum CatalogCommand {
    /// List catalog entities
    List {
        /// Filter by kind (e.g. Component, API, System)
        #[arg(long, short)]
        kind: Option<String>,

        /// Filter by spec.type (e.g. client-account, tenant, service)
        #[arg(long, short = 't')]
        r#type: Option<String>,

        /// Filter by tag (e.g. dev, prod, client)
        #[arg(long)]
        tag: Option<String>,

        /// Filter by namespace
        #[arg(long)]
        namespace: Option<String>,

        /// Sort by field (name, kind, type, owner)
        #[arg(long, short)]
        sort: Option<String>,

        /// Max number of entities to display (client-side, default: 500)
        #[arg(long, default_value = "500")]
        limit: usize,

        /// Skip first N entities (client-side pagination)
        #[arg(long, default_value = "0")]
        offset: usize,

        /// Output format: table, json, or jsonpath=<expr>
        #[arg(long, short, default_value = "table")]
        output: String,
    },
    /// Get a specific entity
    Get {
        /// Entity reference (e.g. component:default/my-service)
        entity_ref: String,

        /// Output format: table, json, or jsonpath=<expr>
        #[arg(long, short, default_value = "table")]
        output: String,
    },
    /// List unique values for a field (kind, spec.type, spec.lifecycle, etc.)
    Facets {
        /// Field to get facets for
        field: String,
    },
    /// Register a new entity location
    Register {
        /// Location URL (e.g. https://github.com/org/repo/blob/main/catalog-info.yaml)
        target: String,
    },
    /// Unregister an entity by removing its location
    Unregister {
        /// Entity reference to unregister
        entity_ref: String,
    },
    /// Refresh a catalog entity
    Refresh {
        /// Entity reference (e.g. component:default/my-service)
        entity_ref: String,
    },
}

#[derive(Deserialize)]
struct Entity {
    #[serde(default)]
    metadata: EntityMetadata,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    spec: EntitySpec,
}

#[derive(Deserialize, Default)]
#[allow(dead_code)]
struct EntitySpec {
    #[serde(default, rename = "type")]
    entity_type: Option<String>,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    lifecycle: Option<String>,
    #[serde(default)]
    system: Option<String>,
}

#[derive(Deserialize, Default)]
struct EntityMetadata {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: Option<String>,
}

pub async fn run(
    client: &BackstageClient,
    command: CatalogCommand,
    plugin_config: &crate::plugin::PluginConfig,
) -> Result<()> {
    match command {
        CatalogCommand::List {
            kind,
            r#type,
            tag,
            namespace,
            sort,
            limit,
            offset,
            output,
        } => {
            list(
                client,
                kind.as_deref(),
                r#type.as_deref(),
                tag.as_deref(),
                namespace.as_deref(),
                sort.as_deref(),
                limit,
                offset,
                &output,
                plugin_config,
            )
            .await
        }
        CatalogCommand::Get { entity_ref, output } => get(client, &entity_ref, &output).await,
        CatalogCommand::Facets { field } => facets(client, &field).await,
        CatalogCommand::Register { target } => register(client, &target).await,
        CatalogCommand::Unregister { entity_ref } => unregister(client, &entity_ref).await,
        CatalogCommand::Refresh { entity_ref } => refresh(client, &entity_ref).await,
    }
}

#[allow(clippy::too_many_arguments)]
async fn list(
    client: &BackstageClient,
    kind: Option<&str>,
    r#type: Option<&str>,
    tag: Option<&str>,
    namespace: Option<&str>,
    sort: Option<&str>,
    limit: usize,
    offset: usize,
    output: &str,
    plugin_config: &crate::plugin::PluginConfig,
) -> Result<()> {
    let mut entities = service::catalog_list(client, kind, r#type, tag, namespace).await?;
    let total = entities.len();

    // Client-side sort
    if let Some(sort_field) = sort {
        entities.sort_by(|a, b| {
            extract_sort_field(a, sort_field).cmp(&extract_sort_field(b, sort_field))
        });
    }

    // Client-side offset + limit
    if offset > 0 {
        entities = entities.into_iter().skip(offset).collect();
    }
    if entities.len() > limit {
        eprintln!(
            "Showing {limit} of {total} entities. Use --limit to show more, --offset to paginate."
        );
        entities.truncate(limit);
    }

    // Output
    if let Some(expr) = output.strip_prefix("jsonpath=") {
        for entity in &entities {
            let value = extract_jsonpath(entity, expr);
            if !value.is_empty() {
                println!("{value}");
            }
        }
        return Ok(());
    }

    match output {
        "json" => println!("{}", serde_json::to_string_pretty(&entities)?),
        _ => {
            let custom_columns = r#type.and_then(|t| plugin_config.columns.get(t));

            if let Some(columns) = custom_columns {
                let mut headers: Vec<&str> = vec!["Name"];
                let col_headers: Vec<String> = columns.iter().map(|c| c.header.clone()).collect();
                for h in &col_headers {
                    headers.push(h);
                }
                headers.push("Description");

                let rows: Vec<Vec<Cell>> = entities
                    .iter()
                    .map(|e| {
                        let name = json_str(e, &["metadata", "name"]);
                        let desc = json_str(e, &["metadata", "description"]);
                        let mut row = vec![Cell::new(name)];
                        for col in columns {
                            let value = col.extract(e);
                            let style = match col.style.as_deref() {
                                Some("env") => display::env_style(&value),
                                _ => Style::Default,
                            };
                            row.push(Cell::styled(value, style));
                        }
                        row.push(Cell::styled(first_line(desc), Style::Dim));
                        row
                    })
                    .collect();
                display::table(&headers, &rows);
            } else {
                let parsed: Vec<Entity> =
                    serde_json::from_value(serde_json::Value::Array(entities))?;
                let rows: Vec<Vec<Cell>> = parsed.iter().map(format_entity_row).collect();
                display::table(&["Name", "Kind", "Type", "Owner", "Description"], &rows);
            }
        }
    }
    Ok(())
}

fn extract_sort_field(entity: &serde_json::Value, field: &str) -> String {
    match field {
        "name" => json_str(entity, &["metadata", "name"]).to_string(),
        "kind" => json_str(entity, &["kind"]).to_string(),
        "type" => json_str(entity, &["spec", "type"]).to_string(),
        "owner" => json_str(entity, &["spec", "owner"]).to_string(),
        _ => json_str(entity, &["metadata", "name"]).to_string(),
    }
}

/// Simple jsonpath extraction: supports dot-separated field paths.
/// Strips leading `$` or `$.` prefix for compatibility with standard jsonpath syntax.
fn extract_jsonpath(entity: &serde_json::Value, expr: &str) -> String {
    let expr = expr
        .strip_prefix("$.")
        .or_else(|| expr.strip_prefix('$'))
        .unwrap_or(expr);
    let mut current = entity;
    for segment in expr.split('.') {
        match current.get(segment) {
            Some(v) => current = v,
            None => return String::new(),
        }
    }
    match current {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn json_str<'a>(v: &'a serde_json::Value, path: &[&str]) -> &'a str {
    let mut current = v;
    for segment in path {
        match current.get(*segment) {
            Some(v) => current = v,
            None => return "",
        }
    }
    current.as_str().unwrap_or("")
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("")
}

fn format_entity_row(e: &Entity) -> Vec<Cell> {
    vec![
        Cell::new(&e.metadata.name),
        Cell::styled(&e.kind, Style::Dim),
        Cell::new(e.spec.entity_type.as_deref().unwrap_or("")),
        Cell::styled(e.spec.owner.as_deref().unwrap_or(""), Style::Dim),
        Cell::styled(
            first_line(e.metadata.description.as_deref().unwrap_or("")),
            Style::Dim,
        ),
    ]
}

async fn get(client: &BackstageClient, entity_ref: &str, output: &str) -> Result<()> {
    let entity = service::catalog_get(client, entity_ref).await?;

    if let Some(expr) = output.strip_prefix("jsonpath=") {
        println!("{}", extract_jsonpath(&entity, expr));
        return Ok(());
    }

    match output {
        "json" => println!("{}", serde_json::to_string_pretty(&entity)?),
        _ => print_entity_detail(&entity),
    }
    Ok(())
}

fn print_entity_detail(entity: &serde_json::Value) {
    let metadata = entity.get("metadata").cloned().unwrap_or_default();
    let spec = entity.get("spec").cloned().unwrap_or_default();

    let kind_str = entity.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    let name_str = metadata.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let desc = metadata
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    println!("\n{} {}", format!("{kind_str}:").dimmed(), name_str.bold());
    if !desc.is_empty() {
        println!("{}", desc.dimmed());
    }

    let mut fields = Vec::new();
    for key in &["type", "lifecycle", "owner", "system"] {
        if let Some(v) = spec.get(*key).and_then(|v| v.as_str()) {
            let label = key[..1].to_uppercase() + &key[1..];
            fields.push((label, v.to_string()));
        }
    }
    if !fields.is_empty() {
        println!();
        for (label, value) in &fields {
            println!("  {:<12} {}", format!("{label}:").dimmed(), value);
        }
    }

    if let Some(annotations) = metadata.get("annotations").and_then(|v| v.as_object()) {
        let custom: Vec<_> = annotations
            .iter()
            .filter(|(k, _)| !k.starts_with("backstage.io/"))
            .collect();
        if !custom.is_empty() {
            println!("\n  {}", "Annotations:".dimmed());
            for (k, v) in custom {
                println!(
                    "    {} {}",
                    format!("{k}:").dimmed(),
                    v.as_str().unwrap_or("")
                );
            }
        }
    }

    if let Some(tags) = metadata.get("tags").and_then(|v| v.as_array())
        && !tags.is_empty()
    {
        let tag_strs: Vec<&str> = tags.iter().filter_map(|t| t.as_str()).collect();
        println!("\n  {} {}", "Tags:".dimmed(), tag_strs.join(", "));
    }

    if let Some(relations) = entity.get("relations").and_then(|v| v.as_array())
        && !relations.is_empty()
    {
        println!("\n  {}", "Relations:".dimmed());
        for rel in relations {
            let rel_type = rel.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let target = rel.get("targetRef").and_then(|v| v.as_str()).unwrap_or("");
            println!("    {} {}", format!("{rel_type}:").dimmed(), target);
        }
    }
    println!();
}

async fn facets(client: &BackstageClient, field: &str) -> Result<()> {
    let resp = service::catalog_facets(client, field).await?;

    if let Some(facets) = resp
        .get("facets")
        .and_then(|f| f.get(field))
        .and_then(|v| v.as_array())
    {
        for facet in facets {
            let value = facet.get("value").and_then(|v| v.as_str()).unwrap_or("");
            let count = facet.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
            println!("{value:<30} {count}");
        }
    } else {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    }
    Ok(())
}

async fn register(client: &BackstageClient, target: &str) -> Result<()> {
    let resp = service::catalog_register(client, target).await?;

    if let Some(location) = resp.get("location") {
        let loc_target = location
            .get("target")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        println!("{} {}", "Registered:".green(), loc_target);
    }
    if let Some(entities) = resp.get("entities").and_then(|v| v.as_array()) {
        for entity in entities {
            let kind = entity.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let name = entity
                .get("metadata")
                .and_then(|m| m.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            println!("  {} {kind}/{name}", "+".green());
        }
    }
    Ok(())
}

async fn unregister(client: &BackstageClient, entity_ref: &str) -> Result<()> {
    let ref_str = service::catalog_unregister(client, entity_ref).await?;
    println!("{} {ref_str}", "Unregistered:".green());
    Ok(())
}

async fn refresh(client: &BackstageClient, entity_ref: &str) -> Result<()> {
    service::catalog_refresh(client, entity_ref).await?;
    println!("{} {entity_ref}", "Refreshed".green());
    Ok(())
}

// Re-export for backward compatibility (used by columns command)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_jsonpath() {
        let entity = serde_json::json!({
            "metadata": {"name": "test", "namespace": "default"},
            "spec": {"type": "service", "owner": "team-a"}
        });
        assert_eq!(extract_jsonpath(&entity, "metadata.name"), "test");
        assert_eq!(extract_jsonpath(&entity, "$.spec.owner"), "team-a");
        assert_eq!(extract_jsonpath(&entity, "spec.missing"), "");
    }
}
