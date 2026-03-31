use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use clap::Subcommand;

use crate::client::BackstageClient;

#[derive(Subcommand)]
pub enum ColumnsCommand {
    /// Auto-generate column definitions from existing entities
    Generate {
        /// Entity type to analyze (e.g. client-account, tenant)
        #[arg(long, short = 't')]
        r#type: String,

        /// Also include standard Backstage annotations (backstage.io/*)
        #[arg(long)]
        include_builtin: bool,
    },
}

pub async fn run(client: &BackstageClient, command: ColumnsCommand) -> Result<()> {
    match command {
        ColumnsCommand::Generate {
            r#type,
            include_builtin,
        } => generate(client, &r#type, include_builtin).await,
    }
}

async fn generate(
    client: &BackstageClient,
    entity_type: &str,
    include_builtin: bool,
) -> Result<()> {
    let path = format!("/api/catalog/entities?filter=spec.type={entity_type}");
    let entities: Vec<serde_json::Value> = client.get(&path).await?;

    if entities.is_empty() {
        eprintln!("No entities found with type '{entity_type}'.");
        return Ok(());
    }

    // Collect all annotation keys across all entities, tracking which have values
    let mut annotation_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut tag_set: BTreeSet<String> = BTreeSet::new();
    let total = entities.len();

    for entity in &entities {
        if let Some(annotations) = entity
            .get("metadata")
            .and_then(|m| m.get("annotations"))
            .and_then(|a| a.as_object())
        {
            for (key, value) in annotations {
                if !include_builtin && key.starts_with("backstage.io/") {
                    continue;
                }
                if value.as_str().is_some_and(|s| !s.is_empty()) {
                    *annotation_counts.entry(key.clone()).or_default() += 1;
                }
            }
        }

        if let Some(tags) = entity
            .get("metadata")
            .and_then(|m| m.get("tags"))
            .and_then(|t| t.as_array())
        {
            for tag in tags {
                if let Some(s) = tag.as_str() {
                    tag_set.insert(s.to_string());
                }
            }
        }
    }

    // Generate YAML output
    eprintln!("Analyzed {} entities of type '{}'\n", total, entity_type);

    println!("{}:", entity_type);
    for (key, count) in &annotation_counts {
        let header = annotation_key_to_header(key);
        let coverage = if *count < total {
            format!("  # {count}/{total} entities")
        } else {
            String::new()
        };
        println!("  - header: {header}");
        println!("    path: metadata.annotations.{key}{coverage}");

        // Suggest style for known patterns
        if key.contains("environment") || key.contains("env") {
            println!("    style: env");
        }
    }

    if !tag_set.is_empty() {
        eprintln!(
            "\nCommon tags: {}",
            tag_set.into_iter().collect::<Vec<_>>().join(", ")
        );
        eprintln!("(Use --tag flag to filter by tag)");
    }

    Ok(())
}

/// Convert an annotation key like "tactna.io/client-account-id" to a readable header
fn annotation_key_to_header(key: &str) -> String {
    // Strip common prefixes
    let stripped = key.rsplit_once('/').map(|(_, name)| name).unwrap_or(key);

    // Convert kebab-case to Title Case
    stripped
        .split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_annotation_key_to_header() {
        assert_eq!(
            annotation_key_to_header("tactna.io/client-account-id"),
            "Client Account Id"
        );
        assert_eq!(annotation_key_to_header("tactna.io/customer"), "Customer");
        assert_eq!(
            annotation_key_to_header("tactna.io/business-account-id-dev"),
            "Business Account Id Dev"
        );
        assert_eq!(
            annotation_key_to_header("some-annotation"),
            "Some Annotation"
        );
    }
}
