use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    tool, tool_handler, tool_router, transport,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::client::BackstageClient;
use crate::plugin::PluginConfig;
use crate::service;

#[derive(Clone)]
pub struct BsctlMcp {
    client: BackstageClient,
    plugin_config: PluginConfig,
    tool_router: ToolRouter<Self>,
}

#[derive(Deserialize, JsonSchema)]
pub struct CatalogListParams {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default, rename = "type")]
    pub entity_type: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct EntityRefParams {
    /// Entity reference (e.g. component:default/my-service)
    pub entity_ref: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct SearchParams {
    pub term: String,
    #[serde(default = "default_limit")]
    pub limit: u32,
}
fn default_limit() -> u32 {
    25
}

#[derive(Deserialize, JsonSchema)]
pub struct TemplateRunParams {
    pub name: String,
    #[serde(default = "default_ns")]
    pub namespace: String,
    #[serde(default)]
    pub values: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize, JsonSchema)]
pub struct TemplateNameParams {
    pub name: String,
    #[serde(default = "default_ns")]
    pub namespace: String,
}
fn default_ns() -> String {
    "default".to_string()
}

#[derive(Deserialize, JsonSchema)]
pub struct TaskIdParams {
    pub task_id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct RegisterParams {
    pub target: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct FacetsParams {
    pub field: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct PluginCallParams {
    pub plugin: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub params: std::collections::HashMap<String, String>,
}

fn to_json(v: &impl serde::Serialize) -> Result<String, String> {
    serde_json::to_string_pretty(v).map_err(|e| e.to_string())
}

#[tool_router]
impl BsctlMcp {
    pub fn new(client: BackstageClient, plugin_config: PluginConfig) -> Self {
        Self {
            client,
            plugin_config,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        name = "catalog_list",
        description = "List entities in the Backstage catalog"
    )]
    async fn catalog_list(&self, params: Parameters<CatalogListParams>) -> Result<String, String> {
        let p = params.0;
        let opts = service::CatalogListOptions {
            kind: p.kind.as_deref(),
            entity_type: p.entity_type.as_deref(),
            tag: None,
            namespace: None,
            limit: Some(500),
            offset: None,
        };
        let entities = service::catalog_list(&self.client, &opts)
            .await
            .map_err(|e| e.to_string())?;
        if let Some(columns) = p
            .entity_type
            .as_ref()
            .and_then(|t| self.plugin_config.columns.get(t))
        {
            let summary: Vec<serde_json::Value> = entities
                .iter()
                .map(|e| {
                    let mut obj = serde_json::Map::new();
                    if let Some(n) = e
                        .get("metadata")
                        .and_then(|m| m.get("name"))
                        .and_then(|v| v.as_str())
                    {
                        obj.insert("name".into(), n.into());
                    }
                    for col in columns {
                        let k = col.header.to_lowercase().replace(' ', "_");
                        let v = col.extract(e);
                        if !v.is_empty() {
                            obj.insert(k, v.into());
                        }
                    }
                    serde_json::Value::Object(obj)
                })
                .collect();
            to_json(&summary)
        } else {
            to_json(&entities)
        }
    }

    #[tool(
        name = "catalog_get",
        description = "Get a specific entity from the catalog"
    )]
    async fn catalog_get(&self, params: Parameters<EntityRefParams>) -> Result<String, String> {
        to_json(
            &service::catalog_get(&self.client, &params.0.entity_ref)
                .await
                .map_err(|e| e.to_string())?,
        )
    }

    #[tool(name = "catalog_refresh", description = "Refresh a catalog entity")]
    async fn catalog_refresh(&self, params: Parameters<EntityRefParams>) -> Result<String, String> {
        service::catalog_refresh(&self.client, &params.0.entity_ref)
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("Refreshed {}", params.0.entity_ref))
    }

    #[tool(
        name = "catalog_register",
        description = "Register a new entity location"
    )]
    async fn catalog_register(&self, params: Parameters<RegisterParams>) -> Result<String, String> {
        to_json(
            &service::catalog_register(&self.client, &params.0.target)
                .await
                .map_err(|e| e.to_string())?,
        )
    }

    #[tool(
        name = "catalog_unregister",
        description = "Unregister an entity from the catalog"
    )]
    async fn catalog_unregister(
        &self,
        params: Parameters<EntityRefParams>,
    ) -> Result<String, String> {
        let r = service::catalog_unregister(&self.client, &params.0.entity_ref)
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("Unregistered {r}"))
    }

    #[tool(
        name = "catalog_facets",
        description = "List unique values for a catalog field"
    )]
    async fn catalog_facets(&self, params: Parameters<FacetsParams>) -> Result<String, String> {
        to_json(
            &service::catalog_facets(&self.client, &params.0.field)
                .await
                .map_err(|e| e.to_string())?,
        )
    }

    #[tool(name = "search", description = "Search the Backstage catalog")]
    async fn search(&self, params: Parameters<SearchParams>) -> Result<String, String> {
        let p = params.0;
        to_json(
            &service::search(&self.client, &p.term, None, p.limit)
                .await
                .map_err(|e| e.to_string())?,
        )
    }

    #[tool(
        name = "template_list",
        description = "List available software templates"
    )]
    async fn template_list(&self) -> Result<String, String> {
        let t = service::template_list(&self.client)
            .await
            .map_err(|e| e.to_string())?;
        let summary: Vec<serde_json::Value> = t
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.get("metadata").and_then(|m| m.get("name")),
                    "title": t.get("metadata").and_then(|m| m.get("title")),
                    "description": t.get("metadata").and_then(|m| m.get("description")),
                })
            })
            .collect();
        to_json(&summary)
    }

    #[tool(
        name = "template_describe",
        description = "Show parameter schema for a template"
    )]
    async fn template_describe(
        &self,
        params: Parameters<TemplateNameParams>,
    ) -> Result<String, String> {
        let p = params.0;
        let e = service::template_describe(&self.client, &p.name, &p.namespace)
            .await
            .map_err(|e| e.to_string())?;
        let spec = e.get("spec").cloned().unwrap_or_default();
        to_json(&serde_json::json!({
            "name": e.get("metadata").and_then(|m| m.get("name")),
            "title": e.get("metadata").and_then(|m| m.get("title")),
            "parameters": spec.get("parameters"),
            "steps": spec.get("steps").and_then(|s| s.as_array()).map(|steps| steps.iter().map(|s| serde_json::json!({"id": s.get("id"), "name": s.get("name"), "action": s.get("action")})).collect::<Vec<_>>()),
        }))
    }

    #[tool(
        name = "template_run",
        description = "Run a Backstage software template"
    )]
    async fn template_run(&self, params: Parameters<TemplateRunParams>) -> Result<String, String> {
        let p = params.0;
        let id = service::template_run(&self.client, &p.name, &p.namespace, p.values)
            .await
            .map_err(|e| e.to_string())?;
        Ok(serde_json::json!({"task_id": id}).to_string())
    }

    #[tool(
        name = "template_status",
        description = "Check status of a scaffolder task"
    )]
    async fn template_status(&self, params: Parameters<TaskIdParams>) -> Result<String, String> {
        to_json(
            &service::template_status(&self.client, &params.0.task_id)
                .await
                .map_err(|e| e.to_string())?,
        )
    }

    #[tool(
        name = "template_cancel",
        description = "Cancel a running scaffolder task"
    )]
    async fn template_cancel(&self, params: Parameters<TaskIdParams>) -> Result<String, String> {
        service::template_cancel(&self.client, &params.0.task_id)
            .await
            .map_err(|e| e.to_string())?;
        Ok(format!("Cancelled task {}", params.0.task_id))
    }

    #[tool(
        name = "login",
        description = "Authenticate with Backstage using guest auth"
    )]
    async fn login(&self) -> Result<String, String> {
        let url = format!("{}/api/auth/guest/refresh", self.client.base_url());
        let resp = reqwest::Client::new()
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Auth failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("Guest auth failed ({})", resp.status()));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Parse failed: {e}"))?;
        let token = body
            .get("backstageIdentity")
            .and_then(|b| b.get("token"))
            .and_then(|t| t.as_str())
            .ok_or("No token in response")?;
        self.client.set_token(token.to_string());
        Ok("Login successful. Guest token is now active.".to_string())
    }

    #[tool(
        name = "plugin_call",
        description = "Call a custom plugin command from .bsctl/plugins.yaml"
    )]
    async fn plugin_call(&self, params: Parameters<PluginCallParams>) -> Result<String, String> {
        let p = params.0;
        let named: Vec<(String, String)> = p.params.into_iter().collect();
        let commands = self
            .plugin_config
            .plugins
            .get(&p.plugin)
            .ok_or_else(|| format!("Unknown plugin: {}", p.plugin))?;
        let cmd = commands
            .get(&p.command)
            .ok_or_else(|| format!("Unknown command: {} {}", p.plugin, p.command))?;

        let mut path = cmd.path.clone();
        for ad in &cmd.args {
            let v = p
                .args
                .get(ad.position - 1)
                .ok_or_else(|| format!("Missing argument: {}", ad.name))?;
            path = path.replace(&format!("{{{}}}", ad.name), &urlencoding::encode(v));
        }
        let mut qp = Vec::new();
        for pd in &cmd.params {
            let v = named.iter().find(|(k, _)| k == &pd.name).map(|(_, v)| v);
            if v.is_none() && pd.required.unwrap_or(false) {
                return Err(format!("Missing parameter: {}", pd.name));
            }
            if let Some(val) = v
                && let Some(qk) = &pd.query
            {
                qp.push(format!(
                    "{}={}",
                    urlencoding::encode(qk),
                    urlencoding::encode(val)
                ));
            }
        }
        if !qp.is_empty() {
            let sep = if path.contains('?') { "&" } else { "?" };
            path = format!("{path}{sep}{}", qp.join("&"));
        }

        match &cmd.method {
            crate::plugin::Method::Get => to_json(
                &self
                    .client
                    .get::<serde_json::Value>(&path)
                    .await
                    .map_err(|e| e.to_string())?,
            ),
            crate::plugin::Method::Delete => {
                let t = self
                    .client
                    .delete_raw(&path)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(if t.is_empty() { "OK".into() } else { t })
            }
            method => {
                let mut bm = serde_json::Map::new();
                for pd in &cmd.params {
                    if let Some(bk) = &pd.body
                        && let Some(val) = named.iter().find(|(k, _)| k == &pd.name).map(|(_, v)| v)
                    {
                        bm.insert(
                            bk.to_string(),
                            serde_json::from_str(val)
                                .unwrap_or_else(|_| serde_json::Value::String(val.to_string())),
                        );
                    }
                }
                let body = serde_json::Value::Object(bm);
                let r: serde_json::Value = match method {
                    crate::plugin::Method::Put => self.client.put(&path, &body).await,
                    _ => self.client.post(&path, &body).await,
                }
                .map_err(|e| e.to_string())?;
                to_json(&r)
            }
        }
    }
}

#[tool_handler]
impl ServerHandler for BsctlMcp {}

pub async fn serve(client: BackstageClient) -> anyhow::Result<()> {
    let plugin_config = PluginConfig::load()?;
    let server = BsctlMcp::new(client, plugin_config);
    let service = server.serve(transport::io::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
