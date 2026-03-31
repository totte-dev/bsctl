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
    pub fn new(base_url: &str, token: Option<&str>) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(30))
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

    async fn request<T: DeserializeOwned>(&self, method: reqwest::Method, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.request(method, &url);
        if let Some(token) = self.current_token() {
            req = req.bearer_auth(token);
        }
        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("API error {status}: {body}");
        }
        Ok(resp.json().await?)
    }

    async fn request_with_body<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.request(method, &url).json(body);
        if let Some(token) = self.current_token() {
            req = req.bearer_auth(token);
        }
        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("API error {status}: {body}");
        }
        Ok(resp.json().await?)
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.request(reqwest::Method::GET, path).await
    }

    pub async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T> {
        self.request_with_body(reqwest::Method::POST, path, body)
            .await
    }

    pub async fn put<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T> {
        self.request_with_body(reqwest::Method::PUT, path, body)
            .await
    }

    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.request(reqwest::Method::DELETE, path).await
    }
}
