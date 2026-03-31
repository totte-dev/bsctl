use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    tool, tool_handler, tool_router, transport,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::client::BackstageClient;
use crate::plugin::PluginConfig;

#[derive(Clone)]
pub struct BsctlMcp {
    client: BackstageClient,
    plugin_config: PluginConfig,
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

#[derive(Deserialize, JsonSchema)]
pub struct CatalogRegisterParams {
    /// Location URL to register (e.g. https://github.com/org/repo/blob/main/catalog-info.yaml)
    pub target: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct CatalogFacetsParams {
    /// Field to get facets for (e.g. kind, spec.type, spec.lifecycle)
    pub field: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct TemplateDescribeParams {
    /// Template name
    pub name: String,
    /// Template namespace (default: default)
    #[serde(default = "default_namespace")]
    pub namespace: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct PluginCallParams {
    /// Plugin name (e.g. terraform, lambda, costs)
    pub plugin: String,
    /// Command name (e.g. prs, status, get)
    pub command: String,
    /// Positional arguments
    #[serde(default)]
    pub args: Vec<String>,
    /// Named parameters as key-value pairs
    #[serde(default)]
    pub params: std::collections::HashMap<String, String>,
}

// -- Tool implementations --

#[tool_router]
impl BsctlMcp {
    pub fn new(client: BackstageClient, plugin_config: PluginConfig) -> Self {
        Self {
            client,
            plugin_config,
            tool_router: Self::tool_router(),
        }
    }

    /// List entities in the Backstage catalog, optionally filtered by kind and type.
    /// When custom columns are defined for the type in .bsctl/columns/, returns
    /// a compact summary with extracted fields instead of full entities.
    #[tool(
        name = "catalog_list",
        description = "List entities in the Backstage catalog. Returns compact summary when custom columns are configured for the type."
    )]
    async fn catalog_list(&self, params: Parameters<CatalogListParams>) -> Result<String, String> {
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
        let entities: Vec<serde_json::Value> = match self
            .client
            .get(&format!("/api/catalog/entities{query}"))
            .await
        {
            Ok(v) => v,
            Err(e) => return Err(e.to_string()),
        };

        // If custom columns are defined for this type, extract a compact summary
        let custom_columns = p
            .entity_type
            .as_ref()
            .and_then(|t| self.plugin_config.columns.get(t));

        if let Some(columns) = custom_columns {
            let summary: Vec<serde_json::Value> = entities
                .iter()
                .map(|e| {
                    let mut obj = serde_json::Map::new();
                    // Always include name
                    if let Some(name) = e
                        .get("metadata")
                        .and_then(|m| m.get("name"))
                        .and_then(|v| v.as_str())
                    {
                        obj.insert("name".into(), name.into());
                    }
                    // Extract custom column values
                    for col in columns {
                        let key = col.header.to_lowercase().replace(' ', "_");
                        let value = col.extract(e);
                        if !value.is_empty() {
                            obj.insert(key, value.into());
                        }
                    }
                    serde_json::Value::Object(obj)
                })
                .collect();
            Ok(serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?)
        } else {
            Ok(serde_json::to_string_pretty(&entities).map_err(|e| e.to_string())?)
        }
    }

    /// Get details of a specific entity by its reference
    #[tool(
        name = "catalog_get",
        description = "Get a specific entity from the Backstage catalog"
    )]
    async fn catalog_get(&self, params: Parameters<CatalogGetParams>) -> Result<String, String> {
        let (kind, namespace, name) = match parse_ref(&params.0.entity_ref) {
            Ok(v) => v,
            Err(e) => return Err(e.to_string()),
        };
        let path = format!("/api/catalog/entities/by-name/{kind}/{namespace}/{name}");
        match self.client.get::<serde_json::Value>(&path).await {
            Ok(v) => Ok(serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Refresh a catalog entity to re-read its source
    #[tool(
        name = "catalog_refresh",
        description = "Refresh a catalog entity to re-read from its source"
    )]
    async fn catalog_refresh(
        &self,
        params: Parameters<CatalogRefreshParams>,
    ) -> Result<String, String> {
        let (kind, namespace, name) = match parse_ref(&params.0.entity_ref) {
            Ok(v) => v,
            Err(e) => return Err(e.to_string()),
        };
        let body = serde_json::json!({ "entityRef": format!("{kind}:{namespace}/{name}") });
        match self
            .client
            .post::<serde_json::Value>("/api/catalog/refresh", &body)
            .await
        {
            Ok(_) => Ok(format!("Refreshed {kind}:{namespace}/{name}")),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Search the Backstage catalog
    #[tool(name = "search", description = "Search the Backstage catalog by term")]
    async fn search(&self, params: Parameters<SearchParams>) -> Result<String, String> {
        let p = params.0;
        let path = format!(
            "/api/search/query?term={}&limit={}",
            urlencoding::encode(&p.term),
            p.limit
        );
        match self.client.get::<serde_json::Value>(&path).await {
            Ok(v) => Ok(serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?),
            Err(e) => Err(e.to_string()),
        }
    }

    /// List available software templates
    #[tool(
        name = "template_list",
        description = "List available software templates in Backstage"
    )]
    async fn template_list(&self) -> Result<String, String> {
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
                    Ok(serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?)
                } else {
                    Ok(serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?)
                }
            }
            Err(e) => Err(e.to_string()),
        }
    }

    /// Run a software template to create infrastructure or resources
    #[tool(
        name = "template_run",
        description = "Run a Backstage software template"
    )]
    async fn template_run(&self, params: Parameters<TemplateRunParams>) -> Result<String, String> {
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
            Ok(v) => Ok(serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Check the status of a running template task
    #[tool(
        name = "template_status",
        description = "Check status of a Backstage scaffolder task"
    )]
    async fn template_status(
        &self,
        params: Parameters<TaskStatusParams>,
    ) -> Result<String, String> {
        let path = format!("/api/scaffolder/v2/tasks/{}", params.0.task_id);
        match self.client.get::<serde_json::Value>(&path).await {
            Ok(v) => Ok(serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Authenticate with Backstage using guest provider (no browser required)
    #[tool(
        name = "login",
        description = "Authenticate with Backstage using guest auth. Call this first if other tools return 401 errors."
    )]
    async fn login(&self) -> Result<String, String> {
        let base_url = self.client.base_url();
        let url = format!("{base_url}/api/auth/guest/refresh");
        let resp = match reqwest::Client::new().get(&url).send().await {
            Ok(r) => r,
            Err(e) => return Err(format!("Failed to reach auth endpoint: {e}")),
        };
        if !resp.status().is_success() {
            return Err(format!(
                "Guest auth failed ({}). Is guest provider enabled?",
                resp.status()
            ));
        }
        let body: serde_json::Value = match resp.json().await {
            Ok(v) => v,
            Err(e) => return Err(format!("Failed to parse auth response: {e}")),
        };
        let token = match body
            .get("backstageIdentity")
            .and_then(|bi| bi.get("token"))
            .and_then(|t| t.as_str())
        {
            Some(t) => t.to_string(),
            None => return Err("No token in guest auth response".to_string()),
        };
        self.client.set_token(token);
        Ok("Login successful. Guest token is now active.".to_string())
    }

    /// Register a new entity location in the catalog
    #[tool(
        name = "catalog_register",
        description = "Register a new entity location in the Backstage catalog"
    )]
    async fn catalog_register(
        &self,
        params: Parameters<CatalogRegisterParams>,
    ) -> Result<String, String> {
        let body = serde_json::json!({
            "type": "url",
            "target": params.0.target,
        });
        match self
            .client
            .post::<serde_json::Value>("/api/catalog/locations", &body)
            .await
        {
            Ok(v) => Ok(serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?),
            Err(e) => Err(e.to_string()),
        }
    }

    /// List unique values for a catalog field (e.g. kind, spec.type)
    #[tool(
        name = "catalog_facets",
        description = "List unique values for a catalog field (e.g. kind, spec.type, spec.lifecycle)"
    )]
    async fn catalog_facets(
        &self,
        params: Parameters<CatalogFacetsParams>,
    ) -> Result<String, String> {
        let path = format!("/api/catalog/entity-facets?facet={}", params.0.field);
        match self.client.get::<serde_json::Value>(&path).await {
            Ok(v) => Ok(serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Show template parameter schema and steps
    #[tool(
        name = "template_describe",
        description = "Show details and parameter schema for a Backstage software template"
    )]
    async fn template_describe(
        &self,
        params: Parameters<TemplateDescribeParams>,
    ) -> Result<String, String> {
        let p = params.0;
        let path = format!(
            "/api/catalog/entities/by-name/template/{}/{}",
            p.namespace, p.name
        );
        match self.client.get::<serde_json::Value>(&path).await {
            Ok(entity) => {
                // Extract just spec.parameters and spec.steps for conciseness
                let spec = entity.get("spec").cloned().unwrap_or_default();
                let summary = serde_json::json!({
                    "name": entity.get("metadata").and_then(|m| m.get("name")),
                    "title": entity.get("metadata").and_then(|m| m.get("title")),
                    "description": entity.get("metadata").and_then(|m| m.get("description")),
                    "parameters": spec.get("parameters"),
                    "steps": spec.get("steps").and_then(|s| s.as_array()).map(|steps| {
                        steps.iter().map(|s| serde_json::json!({
                            "id": s.get("id"),
                            "name": s.get("name"),
                            "action": s.get("action"),
                        })).collect::<Vec<_>>()
                    }),
                });
                Ok(serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?)
            }
            Err(e) => Err(e.to_string()),
        }
    }

    /// Call a custom plugin command defined in .bsctl/plugins.yaml
    #[tool(
        name = "plugin_call",
        description = "Call a custom plugin command (e.g. terraform prs, costs get). See .bsctl/plugins.yaml for available commands."
    )]
    async fn plugin_call(&self, params: Parameters<PluginCallParams>) -> Result<String, String> {
        let p = params.0;
        let named: Vec<(String, String)> = p.params.into_iter().collect();

        // Capture stdout by running the plugin logic and formatting result
        let commands = self
            .plugin_config
            .plugins
            .get(&p.plugin)
            .ok_or_else(|| format!("Unknown plugin: {}", p.plugin))?;

        let cmd = commands
            .get(&p.command)
            .ok_or_else(|| format!("Unknown command: {} {}", p.plugin, p.command))?;

        // Build path with arg substitution
        let mut path = cmd.path.clone();
        for arg_def in &cmd.args {
            let idx = arg_def.position - 1;
            let value = p
                .args
                .get(idx)
                .ok_or_else(|| format!("Missing required argument: {}", arg_def.name))?;
            let encoded = urlencoding::encode(value);
            path = path.replace(&format!("{{{}}}", arg_def.name), &encoded);
        }

        // Build query params
        let mut query_parts = Vec::new();
        for param_def in &cmd.params {
            let value = named
                .iter()
                .find(|(k, _)| k == &param_def.name)
                .map(|(_, v)| v);
            if value.is_none() && param_def.required.unwrap_or(false) {
                return Err(format!("Missing required parameter: {}", param_def.name));
            }
            if let Some(val) = value
                && let Some(query_key) = &param_def.query
            {
                query_parts.push(format!(
                    "{}={}",
                    urlencoding::encode(query_key),
                    urlencoding::encode(val)
                ));
            }
        }
        if !query_parts.is_empty() {
            let sep = if path.contains('?') { "&" } else { "?" };
            path = format!("{path}{sep}{}", query_parts.join("&"));
        }

        match &cmd.method {
            crate::plugin::Method::Get => match self.client.get::<serde_json::Value>(&path).await {
                Ok(v) => Ok(serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?),
                Err(e) => Err(e.to_string()),
            },
            crate::plugin::Method::Delete => match self.client.delete_raw(&path).await {
                Ok(text) => Ok(if text.is_empty() {
                    "OK".to_string()
                } else {
                    text
                }),
                Err(e) => Err(e.to_string()),
            },
            _ => {
                let mut body_map = serde_json::Map::new();
                for param_def in &cmd.params {
                    if let Some(body_key) = &param_def.body
                        && let Some(val) = named
                            .iter()
                            .find(|(k, _)| k == &param_def.name)
                            .map(|(_, v)| v)
                    {
                        let json_val = serde_json::from_str(val)
                            .unwrap_or_else(|_| serde_json::Value::String(val.to_string()));
                        body_map.insert(body_key.to_string(), json_val);
                    }
                }
                let body = serde_json::Value::Object(body_map);
                match self.client.post::<serde_json::Value>(&path, &body).await {
                    Ok(v) => Ok(serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?),
                    Err(e) => Err(e.to_string()),
                }
            }
        }
    }
}

#[tool_handler]
impl ServerHandler for BsctlMcp {}

fn parse_ref(entity_ref: &str) -> anyhow::Result<(String, String, String)> {
    crate::commands::catalog::parse_entity_ref(entity_ref)
}

/// Start the MCP server on stdio
pub async fn serve(client: BackstageClient) -> anyhow::Result<()> {
    let plugin_config = PluginConfig::load()?;
    let server = BsctlMcp::new(client, plugin_config);
    let transport = transport::io::stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;
    Ok(())
}
