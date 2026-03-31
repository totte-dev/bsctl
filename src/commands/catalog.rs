use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;
use serde::Deserialize;

use crate::client::BackstageClient;
use crate::display::{self, Cell, Style};

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

        /// Output format
        #[arg(long, short, default_value = "table")]
        output: OutputFormat,
    },
    /// Get a specific entity
    Get {
        /// Entity reference (e.g. component:default/my-service)
        entity_ref: String,

        /// Output format
        #[arg(long, short, default_value = "table")]
        output: OutputFormat,
    },
    /// Refresh a catalog entity
    Refresh {
        /// Entity reference (e.g. component:default/my-service)
        entity_ref: String,
    },
}

#[derive(Clone, clap::ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
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
    namespace: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

pub async fn run(client: &BackstageClient, command: CatalogCommand) -> Result<()> {
    match command {
        CatalogCommand::List {
            kind,
            r#type,
            tag,
            namespace,
            output,
        } => list(client, kind, r#type, tag, namespace, output).await,
        CatalogCommand::Get { entity_ref, output } => get(client, &entity_ref, output).await,
        CatalogCommand::Refresh { entity_ref } => refresh(client, &entity_ref).await,
    }
}

async fn list(
    client: &BackstageClient,
    kind: Option<String>,
    r#type: Option<String>,
    tag: Option<String>,
    namespace: Option<String>,
    output: OutputFormat,
) -> Result<()> {
    let mut filters = Vec::new();
    if let Some(kind) = &kind {
        filters.push(format!("kind={kind}"));
    }
    if let Some(t) = &r#type {
        filters.push(format!("spec.type={t}"));
    }
    if let Some(tag) = &tag {
        filters.push(format!("metadata.tags={tag}"));
    }
    if let Some(ns) = &namespace {
        filters.push(format!("metadata.namespace={ns}"));
    }

    let query = if filters.is_empty() {
        String::new()
    } else {
        format!("?filter={}", filters.join(","))
    };

    let entities: Vec<Entity> = client.get(&format!("/api/catalog/entities{query}")).await?;

    match output {
        OutputFormat::Table => {
            let rows: Vec<Vec<Cell>> = entities.iter().map(format_entity_row).collect();
            display::table(&["Name", "Kind", "Type", "Owner", "Description"], &rows);
        }
        OutputFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!(
                    entities
                        .iter()
                        .map(|e| serde_json::json!({
                            "kind": e.kind,
                            "type": e.spec.entity_type,
                            "namespace": e.metadata.namespace.as_deref().unwrap_or("default"),
                            "name": e.metadata.name,
                            "description": e.metadata.description,
                        }))
                        .collect::<Vec<_>>()
                ))?
            );
        }
    }
    Ok(())
}

fn format_entity_row(e: &Entity) -> Vec<Cell> {
    let name = Cell::new(&e.metadata.name);
    let kind = Cell::styled(&e.kind, Style::Dim);
    let entity_type = Cell::new(e.spec.entity_type.as_deref().unwrap_or(""));
    let owner = Cell::styled(e.spec.owner.as_deref().unwrap_or(""), Style::Dim);
    let desc = Cell::styled(
        e.metadata
            .description
            .as_deref()
            .unwrap_or("")
            .lines()
            .next()
            .unwrap_or(""),
        Style::Dim,
    );

    vec![name, kind, entity_type, owner, desc]
}

async fn get(client: &BackstageClient, entity_ref: &str, output: OutputFormat) -> Result<()> {
    let (kind, namespace, name) = parse_entity_ref(entity_ref)?;
    let path = format!("/api/catalog/entities/by-name/{kind}/{namespace}/{name}");
    let entity: serde_json::Value = client.get(&path).await?;

    match output {
        OutputFormat::Table => {
            let metadata = entity.get("metadata").cloned().unwrap_or_default();
            let spec = entity.get("spec").cloned().unwrap_or_default();

            // Header
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

            // Spec fields
            let mut fields = Vec::new();
            if let Some(v) = spec.get("type").and_then(|v| v.as_str()) {
                fields.push(("Type", v.to_string()));
            }
            if let Some(v) = spec.get("lifecycle").and_then(|v| v.as_str()) {
                fields.push(("Lifecycle", v.to_string()));
            }
            if let Some(v) = spec.get("owner").and_then(|v| v.as_str()) {
                fields.push(("Owner", v.to_string()));
            }
            if let Some(v) = spec.get("system").and_then(|v| v.as_str()) {
                fields.push(("System", v.to_string()));
            }

            if !fields.is_empty() {
                println!();
                for (label, value) in &fields {
                    println!("  {:<12} {}", format!("{label}:").dimmed(), value);
                }
            }

            // Annotations
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

            // Tags
            if let Some(tags) = metadata.get("tags").and_then(|v| v.as_array())
                && !tags.is_empty()
            {
                let tag_strs: Vec<&str> = tags.iter().filter_map(|t| t.as_str()).collect();
                println!("\n  {} {}", "Tags:".dimmed(), tag_strs.join(", "));
            }

            // Relations
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
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&entity)?);
        }
    }
    Ok(())
}

async fn refresh(client: &BackstageClient, entity_ref: &str) -> Result<()> {
    let (kind, namespace, name) = parse_entity_ref(entity_ref)?;
    let path = format!("/api/catalog/entities/by-name/{kind}/{namespace}/{name}");
    let entity: serde_json::Value = client.get(&path).await?;

    let uid = entity
        .get("metadata")
        .and_then(|m| m.get("uid"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Entity has no UID"))?;

    let body = serde_json::json!({ "entityRef": format!("{kind}:{namespace}/{name}") });
    let _: serde_json::Value = client.post("/api/catalog/refresh", &body).await?;

    println!(
        "{} {kind}:{namespace}/{name} (uid: {uid})",
        "Refreshed".green()
    );
    Ok(())
}

/// Parse entity reference like "component:default/my-service" or "component:my-service"
pub fn parse_entity_ref(entity_ref: &str) -> Result<(String, String, String)> {
    let (kind, rest) = entity_ref.split_once(':').ok_or_else(|| {
        anyhow::anyhow!(
            "Invalid entity reference '{entity_ref}'. Expected format: kind:namespace/name or kind:name"
        )
    })?;

    let (namespace, name) = if let Some((ns, n)) = rest.split_once('/') {
        (ns.to_string(), n.to_string())
    } else {
        ("default".to_string(), rest.to_string())
    };

    Ok((kind.to_lowercase(), namespace, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_entity_ref_full() {
        let (kind, ns, name) = parse_entity_ref("Component:custom-ns/my-service").unwrap();
        assert_eq!(kind, "component");
        assert_eq!(ns, "custom-ns");
        assert_eq!(name, "my-service");
    }

    #[test]
    fn parse_entity_ref_default_namespace() {
        let (kind, ns, name) = parse_entity_ref("API:my-api").unwrap();
        assert_eq!(kind, "api");
        assert_eq!(ns, "default");
        assert_eq!(name, "my-api");
    }

    #[test]
    fn parse_entity_ref_missing_kind() {
        let result = parse_entity_ref("just-a-name");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid entity reference")
        );
    }
}
