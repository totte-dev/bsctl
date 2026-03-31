use std::sync::{Arc, RwLock};

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
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: Arc::new(RwLock::new(token.map(String::from))),
            http: Client::new(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn set_token(&self, token: String) {
        *self.token.write().unwrap() = Some(token);
    }

    fn current_token(&self) -> Option<String> {
        self.token.read().unwrap().clone()
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.get(&url);
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

    pub async fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &impl serde::Serialize,
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.post(&url).json(body);
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
}
