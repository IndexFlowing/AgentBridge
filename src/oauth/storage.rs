// src/oauth/storage.rs
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Serialize, Deserialize)]
pub struct RegisteredClient {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub client_name: String,
    pub redirect_uris: Vec<String>,
    pub token_endpoint_auth_method: String,
}

impl RegisteredClient {
    pub fn redirect_allowed(&self, uri: &str) -> bool {
        if self.redirect_uris.is_empty() {
            return is_allowed_redirect(uri);
        }
        self.redirect_uris.iter().any(|u| u == uri)
    }
}

#[derive(Clone)]
pub struct PendingAuth {
    pub client_id: String,
    pub redirect_uri: String,
    pub state: Option<String>,
    pub code_challenge: String,
    pub scope: String,
    pub resource: Option<String>,
    pub expires_at: u64, // UNIX timestamp in seconds
}

#[derive(Clone)]
pub struct AuthCode {
    pub client_id: String,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub scope: String,
    pub resource: Option<String>,
    pub expires_at: u64, // UNIX timestamp in seconds
}

#[derive(Clone, Serialize, Deserialize)]
pub struct IssuedToken {
    pub client_id: String,
    pub scope: String,
    pub expires_at: u64, // UNIX timestamp in seconds
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn is_allowed_redirect(uri: &str) -> bool {
    if uri.contains(['<', '>', ' ', '\n', '\r', '\\']) {
        return false;
    }
    if let Some(rest) = uri.strip_prefix("https://") {
        return !rest.is_empty();
    }
    if let Some(rest) = uri.strip_prefix("http://") {
        let host = rest.split(['/', '?', '#']).next().unwrap_or("");
        let host = host.rsplit('@').next().unwrap_or(host);
        if let Some(inner) = host.strip_prefix('[') {
            return inner.split(']').next().unwrap_or("") == "::1";
        }
        let hostname = host.split(':').next().unwrap_or(host);
        return matches!(hostname, "127.0.0.1" | "localhost");
    }
    false
}
