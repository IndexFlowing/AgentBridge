pub mod crypto;
pub mod guard;
pub mod http;
pub mod server;
pub mod storage;
pub mod views;

use std::sync::Arc;
use serde::{Deserialize, Serialize};

pub use http::{extract_bearer, mcp_auth_middleware, public_origin, router};
pub use server::{ConnectedClientInfo, OauthServer};

pub const SCOPES_SUPPORTED: &[&str] = &["mcp:read", "mcp:write"];

#[derive(Clone)]
pub struct OauthSettings {
    pub require_auth: bool,
    pub admin_password: String,
    pub password_generated: bool,
    pub static_token: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
}

#[derive(Clone)]
pub struct AuthHttpState {
    pub oauth: Arc<OauthServer>,
    pub listen_base: String,
}

#[derive(Debug, thiserror::Error)]
pub enum OauthError {
    #[error("{0}")]
    InvalidRequest(String),
    #[error("{0}")]
    InvalidClient(String),
    #[error("{0}")]
    InvalidGrant(String),
    #[error("{0}")]
    AccessDenied(String),
    #[error("{0}")]
    ServerError(String),
    #[error("too many failed attempts; try again shortly")]
    Locked,
    #[error("{0}")]
    Redirect(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub refresh_token: String,
    pub scope: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AuthorizeQuery {
    pub response_type: Option<String>,
    pub client_id: Option<String>,
    pub redirect_uri: Option<String>,
    pub state: Option<String>,
    pub scope: Option<String>,
    pub code_challenge: Option<String>,
    pub code_challenge_method: Option<String>,
    pub resource: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApproveForm {
    pub request_id: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub action: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RegisterRequest {
    #[serde(default)]
    pub redirect_uris: Vec<String>,
    pub client_name: Option<String>,
    pub token_endpoint_auth_method: Option<String>,
    #[serde(default)]
    pub grant_types: Vec<String>,
    #[serde(default)]
    pub response_types: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RegisterResponse {
    pub client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    pub client_id_issued_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_name: Option<String>,
    pub redirect_uris: Vec<String>,
    pub grant_types: Vec<String>,
    pub response_types: Vec<String>,
    pub token_endpoint_auth_method: String,
    pub client_secret_expires_at: u64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TokenRequest {
    #[serde(default)]
    pub grant_type: String,
    pub code: Option<String>,
    pub redirect_uri: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub code_verifier: Option<String>,
    pub refresh_token: Option<String>,
    pub resource: Option<String>,
    pub token: Option<String>,
}

pub fn generate_admin_password() -> String {
    let hex = uuid::Uuid::new_v4().simple().to_string();
    format!("{}-{}-{}", &hex[0..4], &hex[4..8], &hex[8..12])
}