use std::sync::{Arc, RwLock};
use std::time::Duration;

use anyhow::Result;
use reqwest::Client;
use serde::de::DeserializeOwned;

#[derive(Clone)]
pub struct BackstageClient {
    base_url: String,
    token: Arc<RwLock<Option<String>>>,
    http: Client,
}

impl BackstageClient {
    pub fn new(base_url: &str, token: Option<&str>, insecure: bool) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
            .danger_accept_invalid_certs(insecure)
            .build()
            .expect("failed to build HTTP client");
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: Arc::new(RwLock::new(token.map(String::from))),
            http,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn set_token(&self, token: String) {
        *self.token.write().expect("token lock poisoned") = Some(token);
    }

    fn current_token(&self) -> Option<String> {
        self.token.read().expect("token lock poisoned").clone()
    }

    fn build_request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.request(method, &url);
        if let Some(token) = self.current_token() {
            req = req.bearer_auth(token);
        }
        req
    }

    async fn send_and_parse<T: DeserializeOwned>(&self, req: reqwest::RequestBuilder) -> Result<T> {
        let resp = req.send().await?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();

        if !status.is_success() {
            anyhow::bail!("{}", format_api_error(status, &body));
        }

        serde_json::from_str(&body).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse response: {e}\nBody: {}",
                truncate(&body, 500)
            )
        })
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let req = self.build_request(reqwest::Method::GET, path);
        self.send_and_parse(req).await
    }

    pub async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T> {
        let req = self.build_request(reqwest::Method::POST, path).json(body);
        self.send_and_parse(req).await
    }

    pub async fn put<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T> {
        let req = self.build_request(reqwest::Method::PUT, path).json(body);
        self.send_and_parse(req).await
    }

    #[allow(dead_code)]
    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let req = self.build_request(reqwest::Method::DELETE, path);
        self.send_and_parse(req).await
    }

    pub async fn delete_raw(&self, path: &str) -> Result<String> {
        let req = self.build_request(reqwest::Method::DELETE, path);
        let resp = req.send().await?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();

        if !status.is_success() {
            anyhow::bail!("{}", format_api_error(status, &body));
        }
        Ok(body)
    }
}

/// Extract a human-readable error message from Backstage API responses.
/// Backstage returns structured errors like:
/// ```json
/// {"error": {"name": "NotFoundError", "message": "Entity not found"}}
/// ```
fn format_api_error(status: reqwest::StatusCode, body: &str) -> String {
    // Try to parse as Backstage structured error
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(msg) = json
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
        {
            let name = json
                .get("error")
                .and_then(|e| e.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");
            return if name.is_empty() {
                format!("{status}: {msg}")
            } else {
                format!("{status} ({name}): {msg}")
            };
        }
    }
    // Fall back to raw body
    format!("{status}: {}", truncate(body, 500))
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() > max { &s[..max] } else { s }
}
