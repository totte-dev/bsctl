use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use clap::Subcommand;

use crate::client::BackstageClient;
use crate::plugin::PluginConfig;

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

        /// Write directly to .bsctl/columns/<type>.yaml
        #[arg(long, short)]
        write: bool,
    },
}

pub async fn run(
    client: &BackstageClient,
    command: ColumnsCommand,
    plugin_config: &PluginConfig,
) -> Result<()> {
    match command {
        ColumnsCommand::Generate {
            r#type,
            include_builtin,
            write,
        } => generate(client, &r#type, include_builtin, write, plugin_config).await,
    }
}

async fn generate(
    client: &BackstageClient,
    entity_type: &str,
    include_builtin: bool,
    write: bool,
    plugin_config: &PluginConfig,
) -> Result<()> {
    let path = format!("/api/catalog/entities?filter=spec.type={entity_type}");
    let entities: Vec<serde_json::Value> = client.get(&path).await?;

    if entities.is_empty() {
        eprintln!("No entities found with type '{entity_type}'.");
        return Ok(());
    }

    // Collect all annotation keys across all entities
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
                if crate::plugin::is_path_ignored(key, &plugin_config.column_ignores) {
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

    eprintln!("Analyzed {} entities of type '{}'", total, entity_type);

    // Build YAML content (list format for per-file columns)
    let mut yaml = String::new();
    for (key, count) in &annotation_counts {
        let header = annotation_key_to_header(key);
        let coverage = if *count < total {
            format!("  # {count}/{total} entities")
        } else {
            String::new()
        };
        yaml.push_str(&format!("- header: {header}\n"));
        yaml.push_str(&format!("  path: metadata.annotations.{key}{coverage}\n"));
        if key.contains("environment") || key.contains("env") {
            yaml.push_str("  style: env\n");
        }
    }

    if write {
        let dir = std::env::current_dir()?.join(".bsctl").join("columns");
        std::fs::create_dir_all(&dir)?;
        let file_path = dir.join(format!("{entity_type}.yaml"));
        std::fs::write(&file_path, &yaml)?;
        eprintln!("Wrote {}", file_path.display());
    } else {
        print!("{yaml}");
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
    let stripped = key.rsplit_once('/').map(|(_, name)| name).unwrap_or(key);

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
    }
}
