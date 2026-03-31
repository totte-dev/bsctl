//! Shared service layer for catalog, search, and template operations.
//! Used by both CLI commands and MCP tools.

use anyhow::Result;

use crate::client::BackstageClient;

// -- Catalog --

pub struct CatalogListOptions<'a> {
    pub kind: Option<&'a str>,
    pub entity_type: Option<&'a str>,
    pub tag: Option<&'a str>,
    pub namespace: Option<&'a str>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

pub async fn catalog_list(
    client: &BackstageClient,
    opts: &CatalogListOptions<'_>,
) -> Result<Vec<serde_json::Value>> {
    let mut filters = Vec::new();
    if let Some(kind) = opts.kind {
        filters.push(format!("kind={kind}"));
    }
    if let Some(t) = opts.entity_type {
        filters.push(format!("spec.type={t}"));
    }
    if let Some(tag) = opts.tag {
        filters.push(format!("metadata.tags={tag}"));
    }
    if let Some(ns) = opts.namespace {
        filters.push(format!("metadata.namespace={ns}"));
    }

    let mut query_params = Vec::new();
    if !filters.is_empty() {
        query_params.push(format!("filter={}", filters.join(",")));
    }
    if let Some(limit) = opts.limit {
        query_params.push(format!("limit={limit}"));
    }
    if let Some(offset) = opts.offset
        && offset > 0
    {
        query_params.push(format!("offset={offset}"));
    }

    let query = if query_params.is_empty() {
        String::new()
    } else {
        format!("?{}", query_params.join("&"))
    };

    client.get(&format!("/api/catalog/entities{query}")).await
}

pub async fn catalog_get(client: &BackstageClient, entity_ref: &str) -> Result<serde_json::Value> {
    let (kind, namespace, name) = parse_entity_ref(entity_ref)?;
    let path = format!("/api/catalog/entities/by-name/{kind}/{namespace}/{name}");
    client.get(&path).await
}

pub async fn catalog_refresh(client: &BackstageClient, entity_ref: &str) -> Result<()> {
    let (kind, namespace, name) = parse_entity_ref(entity_ref)?;
    let body = serde_json::json!({ "entityRef": format!("{kind}:{namespace}/{name}") });
    let _: serde_json::Value = client.post("/api/catalog/refresh", &body).await?;
    Ok(())
}

pub async fn catalog_register(client: &BackstageClient, target: &str) -> Result<serde_json::Value> {
    let body = serde_json::json!({
        "type": "url",
        "target": target,
    });
    client.post("/api/catalog/locations", &body).await
}

pub async fn catalog_unregister(client: &BackstageClient, entity_ref: &str) -> Result<String> {
    let (kind, namespace, name) = parse_entity_ref(entity_ref)?;
    let path = format!("/api/catalog/entities/by-name/{kind}/{namespace}/{name}");
    let entity: serde_json::Value = client.get(&path).await?;

    let location = entity
        .get("metadata")
        .and_then(|m| m.get("annotations"))
        .and_then(|a| a.get("backstage.io/managed-by-location"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Entity has no managed-by-location annotation"))?;

    let locations: Vec<serde_json::Value> = client.get("/api/catalog/locations").await?;
    let location_entry = locations.iter().find(|l| {
        l.get("data")
            .and_then(|d| d.get("target"))
            .and_then(|v| v.as_str())
            .is_some_and(|t| location.ends_with(t))
    });

    if let Some(entry) = location_entry
        && let Some(id) = entry
            .get("data")
            .and_then(|d| d.get("id"))
            .and_then(|v| v.as_str())
    {
        client
            .delete_raw(&format!("/api/catalog/locations/{id}"))
            .await?;
        return Ok(format!("{kind}:{namespace}/{name}"));
    }

    anyhow::bail!("Could not find location for {entity_ref}. Location: {location}");
}

pub async fn catalog_facets(client: &BackstageClient, field: &str) -> Result<serde_json::Value> {
    let path = format!(
        "/api/catalog/entity-facets?facet={}",
        urlencoding::encode(field)
    );
    client.get(&path).await
}

// -- Search --

pub async fn search(
    client: &BackstageClient,
    term: &str,
    search_type: Option<&str>,
    limit: u32,
) -> Result<serde_json::Value> {
    let mut params = vec![
        format!("term={}", urlencoding::encode(term)),
        format!("limit={limit}"),
    ];
    if let Some(t) = search_type {
        params.push(format!("types[0]={}", urlencoding::encode(t)));
    }
    let path = format!("/api/search/query?{}", params.join("&"));
    client.get(&path).await
}

// -- Templates --

pub async fn template_list(client: &BackstageClient) -> Result<Vec<serde_json::Value>> {
    client
        .get("/api/catalog/entities?filter=kind=Template")
        .await
}

pub async fn template_describe(
    client: &BackstageClient,
    name: &str,
    namespace: &str,
) -> Result<serde_json::Value> {
    let path = format!(
        "/api/catalog/entities/by-name/template/{}/{}",
        urlencoding::encode(namespace),
        urlencoding::encode(name)
    );
    client.get(&path).await
}

pub async fn template_run(
    client: &BackstageClient,
    name: &str,
    namespace: &str,
    values: serde_json::Map<String, serde_json::Value>,
) -> Result<String> {
    let body = serde_json::json!({
        "templateRef": format!("template:{namespace}/{name}"),
        "values": values,
    });

    #[derive(serde::Deserialize)]
    struct TaskCreated {
        id: String,
    }

    let resp: TaskCreated = client.post("/api/scaffolder/v2/tasks", &body).await?;
    Ok(resp.id)
}

pub async fn template_status(client: &BackstageClient, task_id: &str) -> Result<serde_json::Value> {
    let path = format!("/api/scaffolder/v2/tasks/{}", urlencoding::encode(task_id));
    client.get(&path).await
}

pub async fn template_cancel(client: &BackstageClient, task_id: &str) -> Result<()> {
    let path = format!(
        "/api/scaffolder/v2/tasks/{}/cancel",
        urlencoding::encode(task_id)
    );
    let body = serde_json::json!({});
    let _: serde_json::Value = client.post(&path, &body).await?;
    Ok(())
}

pub async fn template_events(
    client: &BackstageClient,
    task_id: &str,
) -> Result<Vec<serde_json::Value>> {
    let path = format!(
        "/api/scaffolder/v2/tasks/{}/events",
        urlencoding::encode(task_id)
    );
    client.get(&path).await
}

// -- Shared --

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
        assert!(parse_entity_ref("just-a-name").is_err());
    }
}
