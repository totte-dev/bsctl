use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::Config;

#[derive(Serialize, Deserialize, Default)]
pub struct Credentials {
    #[serde(default)]
    pub tokens: HashMap<String, TokenEntry>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TokenEntry {
    pub token: String,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
}

impl Credentials {
    fn path() -> std::path::PathBuf {
        Config::config_dir().join("credentials.json")
    }

    pub fn load() -> Result<Self> {
        let path = Self::path();
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn save(&self) -> Result<()> {
        let dir = Config::config_dir();
        std::fs::create_dir_all(&dir)?;
        let path = Self::path();
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;

        // Restrict permissions on credentials file (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }

        Ok(())
    }

    pub fn get(&self, context: &str) -> Option<&TokenEntry> {
        self.tokens.get(context)
    }

    pub fn set(&mut self, context: String, entry: TokenEntry) {
        self.tokens.insert(context, entry);
    }
}

/// Run the OAuth browser login flow.
///
/// 1. Start a local HTTP server on a random port
/// 2. Open the browser to Backstage auth URL with a redirect to our local server
/// 3. Backstage completes auth and redirects back with a token
/// 4. Capture the token and save it
pub async fn login(base_url: &str, provider: &str, context_name: &str) -> Result<String> {
    let token = if provider == "guest" {
        login_guest(base_url).await?
    } else {
        login_browser(base_url, provider)?
    };

    save_token(&token, provider, context_name)?;
    println!("Login successful! Token saved for context '{context_name}'.");
    Ok(token)
}

/// Guest auth: directly call the refresh endpoint, no browser needed.
async fn login_guest(base_url: &str) -> Result<String> {
    println!("Authenticating as guest...");
    let url = format!("{base_url}/api/auth/guest/refresh");
    let resp = reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .context("Failed to reach Backstage auth endpoint")?;

    if !resp.status().is_success() {
        anyhow::bail!(
            "Guest auth failed ({}). Is guest provider enabled?",
            resp.status()
        );
    }

    let body: serde_json::Value = resp.json().await?;
    let token = body
        .get("backstageIdentity")
        .and_then(|bi| bi.get("token"))
        .and_then(|t| t.as_str())
        .ok_or_else(|| anyhow::anyhow!("No token in guest auth response"))?;

    Ok(token.to_string())
}

/// Browser-based OAuth flow for real auth providers.
fn login_browser(base_url: &str, provider: &str) -> Result<String> {
    let listener =
        TcpListener::bind("127.0.0.1:0").context("Failed to bind local callback server")?;
    let port = listener.local_addr()?.port();
    let callback_url = format!("http://localhost:{port}/callback");

    let auth_url = format!(
        "{base_url}/api/auth/{provider}/start?env=production&scope=&redirect={}",
        urlencoding::encode(&callback_url)
    );

    println!("Opening browser for authentication...");
    println!("If the browser doesn't open, visit:\n  {auth_url}\n");

    if open::that(&auth_url).is_err() {
        eprintln!("Failed to open browser automatically.");
    }

    println!("Waiting for authentication callback (timeout: 5 minutes)...");

    listener
        .set_nonblocking(false)
        .context("Failed to configure listener")?;
    // Set a 5-minute timeout for the OAuth callback
    let timeout = std::time::Duration::from_secs(300);
    let start = std::time::Instant::now();
    listener
        .set_nonblocking(true)
        .context("Failed to set non-blocking")?;

    let (mut stream, _) = loop {
        match listener.accept() {
            Ok(conn) => break conn,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if start.elapsed() > timeout {
                    anyhow::bail!("Authentication timed out after 5 minutes. Please try again.");
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            Err(e) => return Err(e).context("Failed to accept callback"),
        }
    };
    let reader = BufReader::new(&stream);

    let mut request_line = String::new();
    let mut buf_reader = reader;
    buf_reader.read_line(&mut request_line)?;

    let token = extract_token_from_request(&request_line)?;

    let response_body = r#"<!DOCTYPE html>
<html><head><title>bsctl</title></head>
<body style="font-family: system-ui; display: flex; justify-content: center; align-items: center; height: 100vh; margin: 0;">
<div style="text-align: center;">
<h1>Authenticated!</h1>
<p>You can close this window and return to the terminal.</p>
</div></body></html>"#;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response_body.len(),
        response_body
    );
    stream.write_all(response.as_bytes())?;
    stream.flush()?;

    Ok(token)
}

fn save_token(token: &str, provider: &str, context_name: &str) -> Result<()> {
    let mut creds = Credentials::load()?;
    creds.set(
        context_name.to_string(),
        TokenEntry {
            token: token.to_string(),
            expires_at: None,
            provider: Some(provider.to_string()),
        },
    );
    creds.save()?;

    let mut config = Config::load()?;
    if let Some(ctx) = config.contexts.get_mut(context_name) {
        ctx.token = None;
    }
    config.save()?;
    Ok(())
}

/// Extract token from the HTTP request line.
///
/// Backstage's auth flow may return the token in different ways depending on
/// the provider and configuration. We support:
/// - Query parameter: /callback?token=xxx
/// - Query parameter: /callback?backstageIdentity=xxx (encoded JSON)
fn extract_token_from_request(request_line: &str) -> Result<String> {
    let path = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("Invalid HTTP request"))?;

    let query = path.split_once('?').map(|(_, q)| q).ok_or_else(|| {
        anyhow::anyhow!("No query parameters in callback. Authentication may have failed.")
    })?;

    let params: HashMap<String, String> = query
        .split('&')
        .filter_map(|p| {
            let (k, v) = p.split_once('=')?;
            Some((
                urlencoding::decode(k).ok()?.to_string(),
                urlencoding::decode(v).ok()?.to_string(),
            ))
        })
        .collect();

    // Try different parameter names
    if let Some(token) = params.get("token") {
        return Ok(token.clone());
    }

    if let Some(identity) = params.get("backstageIdentity") {
        // backstageIdentity is a JSON object with a token field
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(identity)
            && let Some(token) = parsed.get("token").and_then(|v| v.as_str())
        {
            return Ok(token.to_string());
        }
    }

    if let Some(code) = params.get("code") {
        // OAuth authorization code — would need to exchange for token
        anyhow::bail!(
            "Received authorization code instead of token. \
             The Backstage instance may need custom configuration for CLI auth flow. \
             Code: {code}"
        );
    }

    anyhow::bail!(
        "Could not extract token from callback. Parameters received: {:?}",
        params.keys().collect::<Vec<_>>()
    );
}

/// Resolve the token for a given context, checking credentials.json.
/// Returns None if no token found or if the token is expired.
pub fn resolve_token(context_name: &str) -> Option<String> {
    let creds = Credentials::load().ok()?;
    let entry = creds.get(context_name)?;

    // Check if the JWT is expired
    if is_token_expired(&entry.token) {
        eprintln!(
            "Token for context '{context_name}' has expired. Run 'bsctl login' to re-authenticate."
        );
        return None;
    }

    // Warn if token is expiring soon (within 5 minutes)
    if let Some(remaining) = token_remaining_secs(&entry.token)
        && remaining < 300
    {
        eprintln!(
            "Warning: Token for context '{context_name}' expires in {} seconds.",
            remaining
        );
    }

    Some(entry.token.clone())
}

/// Decode a JWT payload without verifying the signature.
/// We only need to read the `exp` claim to check expiry.
fn decode_jwt_payload(token: &str) -> Option<serde_json::Value> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }

    use base64::Engine;
    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .ok()?;
    serde_json::from_slice(&payload_bytes).ok()
}

/// Check if a JWT token is expired.
/// Returns false if the token is not a JWT or has no `exp` claim.
fn is_token_expired(token: &str) -> bool {
    token_remaining_secs(token).is_some_and(|r| r <= 0)
}

/// Get remaining seconds until token expiry.
/// Returns None if the token is not a JWT or has no `exp` claim.
fn token_remaining_secs(token: &str) -> Option<i64> {
    let payload = decode_jwt_payload(token)?;
    let exp = payload.get("exp")?.as_i64()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    Some(exp - now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_token_from_query() {
        let request = "GET /callback?token=my-backstage-jwt HTTP/1.1";
        let token = extract_token_from_request(request).unwrap();
        assert_eq!(token, "my-backstage-jwt");
    }

    #[test]
    fn extract_token_url_encoded() {
        let request = "GET /callback?token=eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ1c2VyIn0.abc HTTP/1.1";
        let token = extract_token_from_request(request).unwrap();
        assert!(token.starts_with("eyJ"));
    }

    #[test]
    fn extract_token_missing() {
        let request = "GET /callback?error=access_denied HTTP/1.1";
        let result = extract_token_from_request(request);
        assert!(result.is_err());
    }

    #[test]
    fn extract_token_no_query() {
        let request = "GET /callback HTTP/1.1";
        let result = extract_token_from_request(request);
        assert!(result.is_err());
    }

    #[test]
    fn decode_jwt_valid() {
        // JWT with payload: {"sub":"user:default/taka","exp":9999999999}
        use base64::Engine;
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"HS256"}"#);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"sub":"user:default/taka","exp":9999999999}"#);
        let token = format!("{header}.{payload}.fake-signature");

        let decoded = decode_jwt_payload(&token).unwrap();
        assert_eq!(
            decoded.get("sub").and_then(|v| v.as_str()),
            Some("user:default/taka")
        );
        assert_eq!(
            decoded.get("exp").and_then(|v| v.as_i64()),
            Some(9999999999)
        );
    }

    #[test]
    fn token_not_expired() {
        use base64::Engine;
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"HS256"}"#);
        // Expires in year 2286
        let payload =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"exp":9999999999}"#);
        let token = format!("{header}.{payload}.sig");

        assert!(!is_token_expired(&token));
        assert!(token_remaining_secs(&token).unwrap() > 0);
    }

    #[test]
    fn token_expired() {
        use base64::Engine;
        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"HS256"}"#);
        // Expired in 2020
        let payload =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"exp":1577836800}"#);
        let token = format!("{header}.{payload}.sig");

        assert!(is_token_expired(&token));
        assert!(token_remaining_secs(&token).unwrap() < 0);
    }

    #[test]
    fn static_token_not_expired() {
        // Static tokens (not JWTs) should never be considered expired
        assert!(!is_token_expired("my-static-token-abcdef123456"));
    }

    #[test]
    fn credentials_roundtrip() {
        let mut creds = Credentials::default();
        creds.set(
            "test".to_string(),
            TokenEntry {
                token: "abc123".to_string(),
                expires_at: None,
                provider: Some("github".to_string()),
            },
        );
        let entry = creds.get("test").unwrap();
        assert_eq!(entry.token, "abc123");
        assert_eq!(entry.provider.as_deref(), Some("github"));
    }
}
