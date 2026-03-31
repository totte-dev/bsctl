use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    tool, tool_handler, tool_router, transport,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::client::BackstageClient;

#[derive(Clone)]
pub struct BsctlMcp {
    client: BackstageClient,
    tool_router: ToolRouter<Self>,
}

// -- Tool parameter types --

#[derive(Deserialize, JsonSchema)]
pub struct CatalogListParams {
    /// Filter by entity kind (e.g. Component, Resource, API)
    #[serde(default)]
    pub kind: Option<String>,
    /// Filter by spec.type (e.g. tenant, client-account, service)
    #[serde(default, rename = "type")]
    pub entity_type: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct CatalogGetParams {
    /// Entity reference (e.g. component:default/my-service, resource:client-tc3)
    pub entity_ref: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct SearchParams {
    /// Search term
    pub term: String,
    /// Max results (default 25)
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    25
}

#[derive(Deserialize, JsonSchema)]
pub struct TemplateRunParams {
    /// Template name (e.g. tenant-creation)
    pub name: String,
    /// Template namespace (default: default)
    #[serde(default = "default_namespace")]
    pub namespace: String,
    /// Template parameters as key-value pairs
    #[serde(default)]
    pub values: serde_json::Map<String, serde_json::Value>,
}

fn default_namespace() -> String {
    "default".to_string()
}

#[derive(Deserialize, JsonSchema)]
pub struct TaskStatusParams {
    /// Scaffolder task ID
    pub task_id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct CatalogRefreshParams {
    /// Entity reference to refresh (e.g. component:default/my-service)
    pub entity_ref: String,
}

// -- Tool implementations --

#[tool_router]
impl BsctlMcp {
    pub fn new(client: BackstageClient) -> Self {
        Self {
            client,
            tool_router: Self::tool_router(),
        }
    }

    /// List entities in the Backstage catalog, optionally filtered by kind and type
    #[tool(
        name = "catalog_list",
        description = "List entities in the Backstage catalog"
    )]
    async fn catalog_list(&self, params: Parameters<CatalogListParams>) -> String {
        let p = params.0;
        let mut filters = Vec::new();
        if let Some(kind) = &p.kind {
            filters.push(format!("kind={kind}"));
        }
        if let Some(t) = &p.entity_type {
            filters.push(format!("spec.type={t}"));
        }
        let query = if filters.is_empty() {
            String::new()
        } else {
            format!("?filter={}", filters.join(","))
        };
        match self
            .client
            .get::<serde_json::Value>(&format!("/api/catalog/entities{query}"))
            .await
        {
            Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_else(|e| e.to_string()),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Get details of a specific entity by its reference
    #[tool(
        name = "catalog_get",
        description = "Get a specific entity from the Backstage catalog"
    )]
    async fn catalog_get(&self, params: Parameters<CatalogGetParams>) -> String {
        let (kind, namespace, name) = match parse_ref(&params.0.entity_ref) {
            Ok(v) => v,
            Err(e) => return format!("Error: {e}"),
        };
        let path = format!("/api/catalog/entities/by-name/{kind}/{namespace}/{name}");
        match self.client.get::<serde_json::Value>(&path).await {
            Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_else(|e| e.to_string()),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Refresh a catalog entity to re-read its source
    #[tool(
        name = "catalog_refresh",
        description = "Refresh a catalog entity to re-read from its source"
    )]
    async fn catalog_refresh(&self, params: Parameters<CatalogRefreshParams>) -> String {
        let (kind, namespace, name) = match parse_ref(&params.0.entity_ref) {
            Ok(v) => v,
            Err(e) => return format!("Error: {e}"),
        };
        let body = serde_json::json!({ "entityRef": format!("{kind}:{namespace}/{name}") });
        match self
            .client
            .post::<serde_json::Value>("/api/catalog/refresh", &body)
            .await
        {
            Ok(_) => format!("Refreshed {kind}:{namespace}/{name}"),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Search the Backstage catalog
    #[tool(name = "search", description = "Search the Backstage catalog by term")]
    async fn search(&self, params: Parameters<SearchParams>) -> String {
        let p = params.0;
        let path = format!(
            "/api/search/query?term={}&limit={}",
            urlencoding::encode(&p.term),
            p.limit
        );
        match self.client.get::<serde_json::Value>(&path).await {
            Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_else(|e| e.to_string()),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// List available software templates
    #[tool(
        name = "template_list",
        description = "List available software templates in Backstage"
    )]
    async fn template_list(&self) -> String {
        match self
            .client
            .get::<serde_json::Value>("/api/catalog/entities?filter=kind=Template")
            .await
        {
            Ok(v) => {
                if let Some(arr) = v.as_array() {
                    let summary: Vec<serde_json::Value> = arr
                        .iter()
                        .map(|t| {
                            serde_json::json!({
                                "name": t.get("metadata").and_then(|m| m.get("name")),
                                "title": t.get("metadata").and_then(|m| m.get("title")),
                                "description": t.get("metadata").and_then(|m| m.get("description")),
                            })
                        })
                        .collect();
                    serde_json::to_string_pretty(&summary).unwrap_or_else(|e| e.to_string())
                } else {
                    serde_json::to_string_pretty(&v).unwrap_or_else(|e| e.to_string())
                }
            }
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Run a software template to create infrastructure or resources
    #[tool(
        name = "template_run",
        description = "Run a Backstage software template"
    )]
    async fn template_run(&self, params: Parameters<TemplateRunParams>) -> String {
        let p = params.0;
        let body = serde_json::json!({
            "templateRef": format!("template:{}/{}", p.namespace, p.name),
            "values": p.values,
        });
        match self
            .client
            .post::<serde_json::Value>("/api/scaffolder/v2/tasks", &body)
            .await
        {
            Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_else(|e| e.to_string()),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Check the status of a running template task
    #[tool(
        name = "template_status",
        description = "Check status of a Backstage scaffolder task"
    )]
    async fn template_status(&self, params: Parameters<TaskStatusParams>) -> String {
        let path = format!("/api/scaffolder/v2/tasks/{}", params.0.task_id);
        match self.client.get::<serde_json::Value>(&path).await {
            Ok(v) => serde_json::to_string_pretty(&v).unwrap_or_else(|e| e.to_string()),
            Err(e) => format!("Error: {e}"),
        }
    }

    /// Authenticate with Backstage using guest provider (no browser required)
    #[tool(
        name = "login",
        description = "Authenticate with Backstage using guest auth. Call this first if other tools return 401 errors."
    )]
    async fn login(&self) -> String {
        let base_url = self.client.base_url();
        let url = format!("{base_url}/api/auth/guest/refresh");
        let resp = match reqwest::Client::new().get(&url).send().await {
            Ok(r) => r,
            Err(e) => return format!("Error: Failed to reach auth endpoint: {e}"),
        };
        if !resp.status().is_success() {
            return format!(
                "Error: Guest auth failed ({}). Is guest provider enabled?",
                resp.status()
            );
        }
        let body: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => return format!("Error: Failed to parse auth response: {e}"),
        };
        let token = match body
            .get("backstageIdentity")
            .and_then(|bi| bi.get("token"))
            .and_then(|t| t.as_str())
        {
            Some(t) => t.to_string(),
            None => return "Error: No token in guest auth response".to_string(),
        };
        self.client.set_token(token);
        "Login successful. Guest token is now active.".to_string()
    }
}

#[tool_handler]
impl ServerHandler for BsctlMcp {}

fn parse_ref(entity_ref: &str) -> anyhow::Result<(String, String, String)> {
    crate::commands::catalog::parse_entity_ref(entity_ref)
}

/// Start the MCP server on stdio
pub async fn serve(client: BackstageClient) -> anyhow::Result<()> {
    let server = BsctlMcp::new(client);
    let transport = transport::io::stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;
    Ok(())
}
