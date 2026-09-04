//! Lightweight OAuth 2.1 authorization server for MCP clients.
//!
//! Implements the subset ChatGPT, Gemini Web, and other MCP clients need:
//! Protected Resource Metadata (RFC 9728), Authorization Server Metadata
//! (RFC 8414), Authorization Code + PKCE (S256), refresh tokens, and
//! Dynamic Client Registration (RFC 7591).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axum::body::Bytes;
use axum::extract::{Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const SCOPES_SUPPORTED: &[&str] = &["mcp:read", "mcp:write"];
const ACCESS_TTL: Duration = Duration::from_secs(3600);
const REFRESH_TTL: Duration = Duration::from_secs(30 * 24 * 3600);
const CODE_TTL: Duration = Duration::from_secs(600);
const PENDING_TTL: Duration = Duration::from_secs(600);
const MAX_FAILURES: usize = 8;
const FAILURE_WINDOW: Duration = Duration::from_secs(300);
const LOCKOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct OauthServer {
    inner: Arc<Mutex<Store>>,
    admin_password: String,
    password_generated: bool,
    static_token: Option<String>,
    require_auth: bool,
}

#[derive(Clone)]
pub struct OauthSettings {
    pub require_auth: bool,
    pub admin_password: String,
    pub password_generated: bool,
    pub static_token: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
}

struct Store {
    clients: HashMap<String, RegisteredClient>,
    pending: HashMap<String, PendingAuth>,
    codes: HashMap<String, AuthCode>,
    access: HashMap<String, IssuedToken>,
    refresh: HashMap<String, IssuedToken>,
    failures: Vec<Instant>,
    lock_until: Option<Instant>,
}

#[derive(Clone)]
struct RegisteredClient {
    client_id: String,
    client_secret: Option<String>,
    #[allow(dead_code)]
    client_name: String,
    redirect_uris: Vec<String>,
    #[allow(dead_code)]
    token_endpoint_auth_method: String,
}

#[derive(Clone)]
struct PendingAuth {
    client_id: String,
    redirect_uri: String,
    state: Option<String>,
    code_challenge: String,
    scope: String,
    resource: Option<String>,
    expires_at: Instant,
}

#[derive(Clone)]
struct AuthCode {
    client_id: String,
    redirect_uri: String,
    code_challenge: String,
    scope: String,
    resource: Option<String>,
    expires_at: Instant,
}

#[derive(Clone)]
struct IssuedToken {
    client_id: String,
    scope: String,
    expires_at: Instant,
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

#[derive(Clone)]
pub struct AuthHttpState {
    pub oauth: Arc<OauthServer>,
    pub listen_base: String,
}

impl OauthServer {
    pub fn new(settings: OauthSettings) -> Self {
        let mut clients = HashMap::new();
        if let Some(id) = settings
            .client_id
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
        {
            let secret = settings
                .client_secret
                .as_ref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            clients.insert(
                id.clone(),
                RegisteredClient {
                    client_id: id,
                    client_secret: secret,
                    client_name: "AgentBridge".into(),
                    redirect_uris: Vec::new(),
                    token_endpoint_auth_method: if settings.client_secret.is_some() {
                        "client_secret_post".into()
                    } else {
                        "none".into()
                    },
                },
            );
        }
        Self {
            inner: Arc::new(Mutex::new(Store {
                clients,
                pending: HashMap::new(),
                codes: HashMap::new(),
                access: HashMap::new(),
                refresh: HashMap::new(),
                failures: Vec::new(),
                lock_until: None,
            })),
            admin_password: settings.admin_password,
            password_generated: settings.password_generated,
            static_token: settings.static_token.filter(|s| !s.is_empty()),
            require_auth: settings.require_auth,
        }
    }

    pub fn require_auth(&self) -> bool {
        self.require_auth
    }

    pub fn generated_password(&self) -> Option<&str> {
        if self.password_generated && self.require_auth {
            Some(self.admin_password.as_str())
        } else {
            None
        }
    }

    pub fn has_admin_password(&self) -> bool {
        !self.admin_password.is_empty()
    }

    pub fn has_static_token(&self) -> bool {
        self.static_token.is_some()
    }

    pub fn authorization_server_metadata(&self, issuer: &str) -> serde_json::Value {
        let issuer = issuer.trim_end_matches('/');
        serde_json::json!({
            "issuer": issuer,
            "authorization_endpoint": format!("{issuer}/oauth/authorize"),
            "token_endpoint": format!("{issuer}/oauth/token"),
            "registration_endpoint": format!("{issuer}/oauth/register"),
            "revocation_endpoint": format!("{issuer}/oauth/revoke"),
            "scopes_supported": SCOPES_SUPPORTED,
            "response_types_supported": ["code"],
            "grant_types_supported": ["authorization_code", "refresh_token"],
            "code_challenge_methods_supported": ["S256"],
            "token_endpoint_auth_methods_supported": ["none", "client_secret_post", "client_secret_basic"],
            "revocation_endpoint_auth_methods_supported": ["none", "client_secret_post", "client_secret_basic"],
            "response_modes_supported": ["query"],
            "subject_types_supported": ["public"],
            "id_token_signing_alg_values_supported": ["none"],
            "client_id_metadata_document_supported": false,
        })
    }

    pub fn protected_resource_metadata(&self, issuer: &str) -> serde_json::Value {
        let issuer = issuer.trim_end_matches('/');
        serde_json::json!({
            "resource": format!("{issuer}/mcp"),
            "authorization_servers": [issuer],
            "bearer_methods_supported": ["header"],
            "scopes_supported": SCOPES_SUPPORTED,
        })
    }

    pub fn www_authenticate(&self, issuer: &str) -> HeaderValue {
        let issuer = issuer.trim_end_matches('/');
        let value = format!(
            r#"Bearer realm="mcp", resource_metadata="{issuer}/.well-known/oauth-protected-resource""#
        );
        HeaderValue::from_str(&value).unwrap_or_else(|_| HeaderValue::from_static("Bearer"))
    }

    pub fn validate_bearer(&self, token: &str) -> bool {
        if token.is_empty() {
            return false;
        }
        if let Some(static_token) = &self.static_token {
            if ct_eq(token, static_token) {
                return true;
            }
        }
        let mut store = self.lock();
        store.cleanup();
        store
            .access
            .get(token)
            .is_some_and(|t| t.expires_at > Instant::now())
    }

    pub fn begin_authorize(&self, req: &AuthorizeQuery) -> Result<String, OauthError> {
        if req.response_type.as_deref().unwrap_or("") != "code" {
            return Err(self.redirect_or(
                req,
                "unsupported_response_type",
                "response_type must be code",
            ));
        }
        let client_id = req
            .client_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| OauthError::InvalidRequest("client_id is required".into()))?;
        let redirect_uri = req
            .redirect_uri
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| OauthError::InvalidRequest("redirect_uri is required".into()))?;
        if !is_allowed_redirect(redirect_uri) {
            return Err(OauthError::InvalidRequest(
                "redirect_uri must be https or loopback http".into(),
            ));
        }
        let method = req.code_challenge_method.as_deref().unwrap_or("");
        if method != "S256" {
            return Err(self.redirect_or(
                req,
                "invalid_request",
                "code_challenge_method must be S256",
            ));
        }
        let challenge = req
            .code_challenge
            .as_deref()
            .map(str::trim)
            .filter(|s| s.len() >= 43)
            .ok_or_else(|| {
                self.redirect_or(req, "invalid_request", "code_challenge is required")
            })?;

        let mut store = self.lock();
        store.cleanup();
        let client = store
            .clients
            .get(client_id)
            .cloned()
            .ok_or_else(|| OauthError::InvalidClient("unknown client_id".into()))?;
        if !client.redirect_allowed(redirect_uri) {
            return Err(OauthError::InvalidRequest(
                "redirect_uri is not registered for this client".into(),
            ));
        }

        let request_id = random_token("abq_");
        store.pending.insert(
            request_id.clone(),
            PendingAuth {
                client_id: client.client_id,
                redirect_uri: redirect_uri.to_string(),
                state: req.state.clone(),
                code_challenge: challenge.to_string(),
                scope: normalize_scope(req.scope.as_deref()),
                resource: req.resource.clone(),
                expires_at: Instant::now() + PENDING_TTL,
            },
        );
        Ok(request_id)
    }

    pub fn approve(&self, request_id: &str, password: &str) -> Result<String, OauthError> {
        self.check_password(password)?;
        let mut store = self.lock();
        store.cleanup();
        let pending = store
            .pending
            .remove(request_id)
            .ok_or_else(|| OauthError::InvalidRequest("authorization request expired".into()))?;
        if pending.expires_at <= Instant::now() {
            return Err(OauthError::InvalidRequest(
                "authorization request expired".into(),
            ));
        }
        let code = random_token("abc_");
        let redirect = redirect_success(&pending.redirect_uri, &code, pending.state.as_deref());
        store.codes.insert(
            code,
            AuthCode {
                client_id: pending.client_id,
                redirect_uri: pending.redirect_uri,
                code_challenge: pending.code_challenge,
                scope: pending.scope,
                resource: pending.resource,
                expires_at: Instant::now() + CODE_TTL,
            },
        );
        Ok(redirect)
    }

    pub fn deny(&self, request_id: &str) -> Result<String, OauthError> {
        let mut store = self.lock();
        let pending = store
            .pending
            .remove(request_id)
            .ok_or_else(|| OauthError::InvalidRequest("authorization request expired".into()))?;
        Ok(redirect_error(
            &pending.redirect_uri,
            "access_denied",
            "The user denied the request",
            pending.state.as_deref(),
        ))
    }

    pub fn register_client(&self, body: RegisterRequest) -> Result<RegisterResponse, OauthError> {
        if body.redirect_uris.is_empty() {
            return Err(OauthError::InvalidRequest(
                "redirect_uris is required".into(),
            ));
        }
        for uri in &body.redirect_uris {
            if !is_allowed_redirect(uri) {
                return Err(OauthError::InvalidRequest(format!(
                    "redirect_uri not allowed: {uri}"
                )));
            }
        }
        let client_id = random_token("abcid_");
        let method = body
            .token_endpoint_auth_method
            .as_deref()
            .unwrap_or("none")
            .to_string();
        let secret = if method == "none" {
            None
        } else {
            Some(random_token("abcs_"))
        };
        let issued_at = unix_now();
        let client_name = body
            .client_name
            .clone()
            .unwrap_or_else(|| "MCP Client".into());
        let client = RegisteredClient {
            client_id: client_id.clone(),
            client_secret: secret.clone(),
            client_name,
            redirect_uris: body.redirect_uris.clone(),
            token_endpoint_auth_method: method.clone(),
        };
        self.lock().clients.insert(client_id.clone(), client);
        Ok(RegisterResponse {
            client_id,
            client_secret: secret,
            client_id_issued_at: issued_at,
            client_name: body.client_name,
            redirect_uris: body.redirect_uris,
            grant_types: vec!["authorization_code".into(), "refresh_token".into()],
            response_types: vec!["code".into()],
            token_endpoint_auth_method: method,
            client_secret_expires_at: 0,
        })
    }

    pub fn exchange_token(&self, req: TokenRequest) -> Result<TokenResponse, OauthError> {
        match req.grant_type.as_str() {
            "authorization_code" => self.exchange_code(req),
            "refresh_token" => self.exchange_refresh(req),
            other => Err(OauthError::InvalidRequest(format!(
                "unsupported grant_type: {other}"
            ))),
        }
    }

    pub fn revoke(&self, token: &str) {
        let mut store = self.lock();
        store.access.remove(token);
        store.refresh.remove(token);
    }

    fn exchange_code(&self, req: TokenRequest) -> Result<TokenResponse, OauthError> {
        let code = req
            .code
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| OauthError::InvalidRequest("code is required".into()))?;
        let verifier = req
            .code_verifier
            .as_deref()
            .filter(|s| (43..=128).contains(&s.len()))
            .ok_or_else(|| OauthError::InvalidRequest("code_verifier is required".into()))?;
        let redirect_uri = req
            .redirect_uri
            .as_deref()
            .ok_or_else(|| OauthError::InvalidRequest("redirect_uri is required".into()))?;

        let mut store = self.lock();
        store.cleanup();
        let client = self.authenticate_client(&store, &req)?;
        let issued = store
            .codes
            .remove(code)
            .ok_or_else(|| OauthError::InvalidGrant("authorization code is invalid".into()))?;
        if issued.expires_at <= Instant::now() {
            return Err(OauthError::InvalidGrant(
                "authorization code expired".into(),
            ));
        }
        if issued.client_id != client.client_id {
            return Err(OauthError::InvalidGrant("code was issued to another client".into()));
        }
        if issued.redirect_uri != redirect_uri {
            return Err(OauthError::InvalidGrant("redirect_uri mismatch".into()));
        }
        if pkce_challenge(verifier) != issued.code_challenge {
            return Err(OauthError::InvalidGrant("PKCE verification failed".into()));
        }
        if let (Some(expected), Some(got)) = (&issued.resource, &req.resource) {
            if expected != got {
                return Err(OauthError::InvalidGrant("resource mismatch".into()));
            }
        }
        Ok(issue_tokens(&mut store, &client.client_id, &issued.scope))
    }

    fn exchange_refresh(&self, req: TokenRequest) -> Result<TokenResponse, OauthError> {
        let refresh = req
            .refresh_token
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| OauthError::InvalidRequest("refresh_token is required".into()))?;
        let mut store = self.lock();
        store.cleanup();
        let client = self.authenticate_client(&store, &req)?;
        let issued = store
            .refresh
            .remove(refresh)
            .ok_or_else(|| OauthError::InvalidGrant("refresh_token is invalid".into()))?;
        if issued.expires_at <= Instant::now() {
            return Err(OauthError::InvalidGrant("refresh_token expired".into()));
        }
        if issued.client_id != client.client_id {
            return Err(OauthError::InvalidGrant(
                "refresh_token was issued to another client".into(),
            ));
        }
        Ok(issue_tokens(&mut store, &client.client_id, &issued.scope))
    }

    fn authenticate_client<'a>(
        &self,
        store: &'a Store,
        req: &TokenRequest,
    ) -> Result<RegisteredClient, OauthError> {
        let client_id = req
            .client_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| OauthError::InvalidClient("client_id is required".into()))?;
        let client = store
            .clients
            .get(client_id)
            .cloned()
            .ok_or_else(|| OauthError::InvalidClient("unknown client_id".into()))?;
        if let Some(expected) = &client.client_secret {
            let provided = req.client_secret.as_deref().unwrap_or("");
            if !ct_eq(provided, expected) {
                return Err(OauthError::InvalidClient("invalid client_secret".into()));
            }
        }
        Ok(client)
    }

    fn check_password(&self, password: &str) -> Result<(), OauthError> {
        let mut store = self.lock();
        let now = Instant::now();
        if store.lock_until.is_some_and(|t| t > now) {
            return Err(OauthError::Locked);
        }
        store.failures.retain(|t| now.duration_since(*t) < FAILURE_WINDOW);
        if ct_eq(password.trim(), &self.admin_password) && !self.admin_password.is_empty() {
            store.failures.clear();
            return Ok(());
        }
        store.failures.push(now);
        if store.failures.len() >= MAX_FAILURES {
            store.lock_until = Some(now + LOCKOUT);
            store.failures.clear();
            return Err(OauthError::Locked);
        }
        Err(OauthError::AccessDenied("invalid password".into()))
    }

    fn redirect_or(&self, req: &AuthorizeQuery, error: &str, desc: &str) -> OauthError {
        if let (Some(uri), Some(client_id)) = (
            req.redirect_uri.as_deref(),
            req.client_id.as_deref(),
        ) {
            if is_allowed_redirect(uri) {
                let store = self.lock();
                if store
                    .clients
                    .get(client_id)
                    .is_some_and(|c| c.redirect_allowed(uri))
                {
                    return OauthError::Redirect(redirect_error(
                        uri,
                        error,
                        desc,
                        req.state.as_deref(),
                    ));
                }
            }
        }
        OauthError::InvalidRequest(desc.to_string())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Store> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl RegisteredClient {
    fn redirect_allowed(&self, uri: &str) -> bool {
        if self.redirect_uris.is_empty() {
            return is_allowed_redirect(uri);
        }
        self.redirect_uris.iter().any(|u| u == uri)
    }
}

impl Store {
    fn cleanup(&mut self) {
        let now = Instant::now();
        self.pending.retain(|_, v| v.expires_at > now);
        self.codes.retain(|_, v| v.expires_at > now);
        self.access.retain(|_, v| v.expires_at > now);
        self.refresh.retain(|_, v| v.expires_at > now);
    }
}

fn issue_tokens(store: &mut Store, client_id: &str, scope: &str) -> TokenResponse {
    let access = random_token("abt_");
    let refresh = random_token("abr_");
    store.access.insert(
        access.clone(),
        IssuedToken {
            client_id: client_id.to_string(),
            scope: scope.to_string(),
            expires_at: Instant::now() + ACCESS_TTL,
        },
    );
    store.refresh.insert(
        refresh.clone(),
        IssuedToken {
            client_id: client_id.to_string(),
            scope: scope.to_string(),
            expires_at: Instant::now() + REFRESH_TTL,
        },
    );
    TokenResponse {
        access_token: access,
        token_type: "Bearer".into(),
        expires_in: ACCESS_TTL.as_secs(),
        refresh_token: refresh,
        scope: scope.to_string(),
    }
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
    let hex = Uuid::new_v4().simple().to_string();
    format!("{}-{}-{}", &hex[0..4], &hex[4..8], &hex[8..12])
}

pub fn public_origin(headers: &HeaderMap, fallback: &str) -> String {
    let host = header_csv(headers, "x-forwarded-host")
        .or_else(|| header_csv(headers, "host"))
        .map(|h| h.trim().to_string());
    let forwarded_proto = header_csv(headers, "x-forwarded-proto").map(|p| p.trim().to_string());
    let host = match host {
        Some(h) => h,
        None => {
            return fallback.trim_end_matches('/').to_string();
        }
    };
    let proto = if let Some(p) = forwarded_proto {
        p
    } else if is_loopback_host(&host) {
        if fallback.starts_with("https://") {
            "https".into()
        } else {
            "http".into()
        }
    } else {
        "https".into()
    };
    format!("{proto}://{host}")
}

pub fn extract_bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?.trim();
    if value.len() >= 7 && value[..7].eq_ignore_ascii_case("Bearer ") {
        Some(value[7..].trim())
    } else {
        None
    }
}

pub fn router() -> Router<AuthHttpState> {
    Router::new()
        .route(
            "/.well-known/oauth-protected-resource",
            get(protected_resource),
        )
        .route(
            "/.well-known/oauth-protected-resource/{*rest}",
            get(protected_resource),
        )
        .route(
            "/.well-known/oauth-authorization-server",
            get(authorization_server),
        )
        .route(
            "/.well-known/oauth-authorization-server/{*rest}",
            get(authorization_server),
        )
        .route("/.well-known/openid-configuration", get(authorization_server))
        .route(
            "/.well-known/openid-configuration/{*rest}",
            get(authorization_server),
        )
        .route("/oauth/authorize", get(authorize_get).post(authorize_post))
        .route("/authorize", get(authorize_get).post(authorize_post))
        .route("/oauth/token", post(token_post))
        .route("/token", post(token_post))
        .route("/oauth/register", post(register_post))
        .route("/register", post(register_post))
        .route("/oauth/revoke", post(revoke_post))
}

pub async fn mcp_auth_middleware(
    State(state): State<AuthHttpState>,
    req: Request,
    next: axum::middleware::Next,
) -> Response {
    if req.method() == Method::OPTIONS {
        return next.run(req).await;
    }
    if !state.oauth.require_auth() {
        return next.run(req).await;
    }
    let origin = public_origin(req.headers(), &state.listen_base);
    let token = extract_bearer(req.headers());
    if token.is_some_and(|t| state.oauth.validate_bearer(t)) {
        return next.run(req).await;
    }
    unauthorized(&state.oauth, &origin)
}

fn unauthorized(oauth: &OauthServer, origin: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [
            (header::WWW_AUTHENTICATE, oauth.www_authenticate(origin)),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            ),
        ],
        r#"{"error":"invalid_token","error_description":"missing or invalid bearer token"}"#,
    )
        .into_response()
}

async fn protected_resource(State(state): State<AuthHttpState>, req: Request) -> Response {
    let origin = public_origin(req.headers(), &state.listen_base);
    json_meta(state.oauth.protected_resource_metadata(&origin))
}

async fn authorization_server(State(state): State<AuthHttpState>, req: Request) -> Response {
    let origin = public_origin(req.headers(), &state.listen_base);
    json_meta(state.oauth.authorization_server_metadata(&origin))
}

async fn authorize_get(
    State(state): State<AuthHttpState>,
    Query(query): Query<AuthorizeQuery>,
) -> Response {
    if query.client_id.is_none() && query.redirect_uri.is_none() {
        return Html(idle_page()).into_response();
    }
    match state.oauth.begin_authorize(&query) {
        Ok(request_id) => {
            let client = query.client_id.as_deref().unwrap_or("MCP client");
            Html(consent_page(&request_id, client, query.scope.as_deref())).into_response()
        }
        Err(OauthError::Redirect(url)) => Redirect::to(&url).into_response(),
        Err(err) => oauth_html_error(StatusCode::BAD_REQUEST, &err.to_string()),
    }
}

async fn authorize_post(
    State(state): State<AuthHttpState>,
    Form(form): Form<ApproveForm>,
) -> Response {
    let action = form.action.to_ascii_lowercase();
    if action == "deny" {
        return match state.oauth.deny(&form.request_id) {
            Ok(url) => Redirect::to(&url).into_response(),
            Err(err) => oauth_html_error(StatusCode::BAD_REQUEST, &err.to_string()),
        };
    }
    match state.oauth.approve(&form.request_id, &form.password) {
        Ok(url) => Redirect::to(&url).into_response(),
        Err(OauthError::AccessDenied(_)) => Html(consent_page_error(
            &form.request_id,
            "MCP client",
            "Incorrect password. Try again.",
        ))
        .into_response(),
        Err(OauthError::Locked) => {
            oauth_html_error(StatusCode::TOO_MANY_REQUESTS, &OauthError::Locked.to_string())
        }
        Err(err) => oauth_html_error(StatusCode::BAD_REQUEST, &err.to_string()),
    }
}

async fn token_post(
    State(state): State<AuthHttpState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let mut req = match parse_token_body(&headers, &body) {
        Ok(r) => r,
        Err(err) => return token_error(StatusCode::BAD_REQUEST, "invalid_request", &err),
    };
    apply_basic_auth(&headers, &mut req);
    match state.oauth.exchange_token(req) {
        Ok(tokens) => (
            StatusCode::OK,
            [
                (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
                (header::PRAGMA, HeaderValue::from_static("no-cache")),
            ],
            Json(tokens),
        )
            .into_response(),
        Err(OauthError::InvalidClient(msg)) => {
            token_error(StatusCode::UNAUTHORIZED, "invalid_client", &msg)
        }
        Err(OauthError::InvalidGrant(msg)) => {
            token_error(StatusCode::BAD_REQUEST, "invalid_grant", &msg)
        }
        Err(err) => token_error(StatusCode::BAD_REQUEST, "invalid_request", &err.to_string()),
    }
}

async fn register_post(
    State(state): State<AuthHttpState>,
    Json(body): Json<RegisterRequest>,
) -> Response {
    match state.oauth.register_client(body) {
        Ok(resp) => (StatusCode::CREATED, Json(resp)).into_response(),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_client_metadata",
                "error_description": err.to_string(),
            })),
        )
            .into_response(),
    }
}

async fn revoke_post(State(state): State<AuthHttpState>, headers: HeaderMap, body: Bytes) -> Response {
    let mut req = parse_token_body(&headers, &body).unwrap_or_default();
    apply_basic_auth(&headers, &mut req);
    if let Some(token) = req.token.or(req.refresh_token).or(req.code) {
        state.oauth.revoke(&token);
    }
    StatusCode::OK.into_response()
}

fn parse_token_body(headers: &HeaderMap, body: &Bytes) -> Result<TokenRequest, String> {
    let ct = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if ct.contains("application/json") {
        serde_json::from_slice(body).map_err(|e| e.to_string())
    } else {
        serde_urlencoded::from_bytes(body).map_err(|e| e.to_string())
    }
}

fn apply_basic_auth(headers: &HeaderMap, req: &mut TokenRequest) {
    let Some(value) = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) else {
        return;
    };
    let Some(encoded) = value
        .strip_prefix("Basic ")
        .or_else(|| value.strip_prefix("basic "))
    else {
        return;
    };
    let Ok(bytes) = STANDARD.decode(encoded.trim()) else {
        return;
    };
    let Ok(pair) = String::from_utf8(bytes) else {
        return;
    };
    if let Some((id, secret)) = pair.split_once(':') {
        if req.client_id.is_none() {
            req.client_id = Some(id.to_string());
        }
        if req.client_secret.is_none() {
            req.client_secret = Some(secret.to_string());
        }
    }
}

fn json_meta(value: serde_json::Value) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        value.to_string(),
    )
        .into_response()
}

fn token_error(status: StatusCode, error: &str, description: &str) -> Response {
    (
        status,
        [(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))],
        Json(serde_json::json!({
            "error": error,
            "error_description": description,
        })),
    )
        .into_response()
}

fn oauth_html_error(status: StatusCode, message: &str) -> Response {
    (
        status,
        Html(format!(
            "<!doctype html><meta charset=utf-8><title>AgentBridge</title>\
             <body style=\"font-family:sans-serif;max-width:36rem;margin:4rem auto;color:#1a1a1a\">\
             <h1>Authorization failed</h1><p>{}</p></body>",
            html_escape(message)
        )),
    )
        .into_response()
}

fn consent_page(request_id: &str, client: &str, scope: Option<&str>) -> String {
    consent_page_inner(request_id, client, scope, None)
}

fn consent_page_error(request_id: &str, client: &str, error: &str) -> String {
    consent_page_inner(request_id, client, None, Some(error))
}

fn consent_page_inner(
    request_id: &str,
    client: &str,
    scope: Option<&str>,
    error: Option<&str>,
) -> String {
    let scopes = scope.unwrap_or("mcp:read mcp:write");
    let error_html = error
        .map(|e| {
            format!(
                "<p class=\"err\">{}</p>",
                html_escape(e)
            )
        })
        .unwrap_or_default();
    format!(
        r##"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8"/>
  <meta name="viewport" content="width=device-width, initial-scale=1"/>
  <title>Authorize AgentBridge</title>
  <style>
    :root {{ color-scheme: light dark; }}
    body {{
      margin: 0; min-height: 100vh; display: grid; place-items: center;
      font-family: ui-sans-serif, system-ui, sans-serif;
      background: #0f1419; color: #e7ecf1;
    }}
    .card {{
      width: min(28rem, calc(100vw - 2rem));
      background: #1a222c; border: 1px solid #2c3846; border-radius: 16px;
      padding: 1.75rem 1.5rem 1.5rem; box-shadow: 0 20px 50px rgba(0,0,0,.35);
    }}
    h1 {{ font-size: 1.2rem; margin: 0 0 .35rem; }}
    p {{ color: #b7c2cc; line-height: 1.45; }}
    .scopes {{ font-family: ui-monospace, monospace; font-size: .85rem; color: #8bd4c2; }}
    label {{ display: block; font-size: .85rem; margin: 1rem 0 .35rem; color: #d5dde4; }}
    input[type=password] {{
      width: 100%; box-sizing: border-box; padding: .7rem .8rem; border-radius: 10px;
      border: 1px solid #3a4a5c; background: #10161d; color: inherit; font-size: 1rem;
    }}
    .actions {{ display: flex; gap: .6rem; margin-top: 1.2rem; }}
    button {{
      flex: 1; padding: .7rem 1rem; border-radius: 10px; border: 0; cursor: pointer;
      font-weight: 600; font-size: .95rem;
    }}
    .ok {{ background: #3dd6c6; color: #06231f; }}
    .no {{ background: #2a3440; color: #d5dde4; }}
    .err {{ color: #ff8d8d; font-weight: 600; }}
    .brand {{ letter-spacing: .08em; text-transform: uppercase; font-size: .72rem; color: #7f8b98; margin-bottom: .8rem; }}
  </style>
</head>
<body>
  <form class="card" method="post" action="/oauth/authorize">
    <div class="brand">AgentBridge MCP</div>
    <h1>Approve access</h1>
    <p><strong>{client}</strong> wants to use this workspace through MCP.</p>
    <p class="scopes">{scopes}</p>
    {error}
    <input type="hidden" name="request_id" value="{rid}"/>
    <label for="password">Admin password</label>
    <input id="password" name="password" type="password" autocomplete="current-password" required autofocus/>
    <div class="actions">
      <button class="no" type="submit" name="action" value="deny" formnovalidate>Deny</button>
      <button class="ok" type="submit" name="action" value="approve">Approve</button>
    </div>
  </form>
</body>
</html>
"##,
        client = html_escape(client),
        scopes = html_escape(scopes),
        error = error_html,
        rid = html_escape(request_id),
    )
}

fn idle_page() -> String {
    r#"<!doctype html><meta charset=utf-8><title>AgentBridge OAuth</title>
<body style="font-family:ui-sans-serif,system-ui,sans-serif;max-width:36rem;margin:4rem auto;color:#e7ecf1;background:#0f1419">
<h1>AgentBridge authorization</h1>
<p>ChatGPT and Gemini open this page during the MCP OAuth 2.1 flow. Start a custom MCP connection against <code>/mcp</code> to begin.</p>
</body>"#
        .into()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn is_allowed_redirect(uri: &str) -> bool {
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
            let ipv6 = inner.split(']').next().unwrap_or("");
            return ipv6 == "::1";
        }
        let hostname = host.split(':').next().unwrap_or(host);
        return matches!(hostname, "127.0.0.1" | "localhost");
    }
    false
}

fn is_loopback_host(host: &str) -> bool {
    let host = host.split(':').next().unwrap_or(host);
    matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]")
}

fn header_csv<'a>(headers: &'a HeaderMap, name: &'static str) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn normalize_scope(scope: Option<&str>) -> String {
    let requested = scope.unwrap_or("mcp:read mcp:write");
    let mut out = Vec::new();
    for part in requested.split_whitespace() {
        if SCOPES_SUPPORTED.contains(&part) && !out.contains(&part) {
            out.push(part);
        }
    }
    if out.is_empty() {
        "mcp:read mcp:write".into()
    } else {
        out.join(" ")
    }
}

fn pkce_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

fn random_token(prefix: &str) -> String {
    format!(
        "{prefix}{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn ct_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn redirect_success(uri: &str, code: &str, state: Option<&str>) -> String {
    append_query(
        uri,
        &[
            ("code", Some(code)),
            ("state", state),
        ],
    )
}

fn redirect_error(uri: &str, error: &str, desc: &str, state: Option<&str>) -> String {
    append_query(
        uri,
        &[
            ("error", Some(error)),
            ("error_description", Some(desc)),
            ("state", state),
        ],
    )
}

fn append_query(uri: &str, pairs: &[(&str, Option<&str>)]) -> String {
    let mut out = uri.to_string();
    let mut first = !uri.contains('?');
    for (k, v) in pairs {
        let Some(v) = v else { continue };
        out.push(if first { '?' } else { '&' });
        first = false;
        out.push_str(k);
        out.push('=');
        out.push_str(&form_urlencoded(v));
    }
    out
}

fn form_urlencoded(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> OauthServer {
        OauthServer::new(OauthSettings {
            require_auth: true,
            admin_password: "secret-pin".into(),
            password_generated: false,
            static_token: Some("static-token".into()),
            client_id: Some("fixed".into()),
            client_secret: Some("fixed-secret".into()),
        })
    }

    fn pkce_pair() -> (String, String) {
        let verifier = "a".repeat(64);
        (verifier.clone(), pkce_challenge(&verifier))
    }

    #[test]
    fn pkce_s256_roundtrip() {
        let (v, c) = pkce_pair();
        assert_eq!(pkce_challenge(&v), c);
        assert_ne!(pkce_challenge("b".repeat(64).as_str()), c);
    }

    #[test]
    fn static_bearer_accepted() {
        let s = server();
        assert!(s.validate_bearer("static-token"));
        assert!(!s.validate_bearer("nope"));
    }

    #[test]
    fn authorization_code_pkce_flow() {
        let s = server();
        let (verifier, challenge) = pkce_pair();
        let q = AuthorizeQuery {
            response_type: Some("code".into()),
            client_id: Some("fixed".into()),
            redirect_uri: Some("https://chatgpt.com/connector_platform_oauth_redirect".into()),
            state: Some("xyz".into()),
            scope: Some("mcp:read mcp:write".into()),
            code_challenge: Some(challenge),
            code_challenge_method: Some("S256".into()),
            resource: Some("https://example.com/mcp".into()),
        };
        let rid = s.begin_authorize(&q).unwrap();
        let loc = s.approve(&rid, "secret-pin").unwrap();
        assert!(loc.contains("code="));
        assert!(loc.contains("state=xyz"));
        let code = loc
            .split("code=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .to_string();
        let tokens = s
            .exchange_token(TokenRequest {
                grant_type: "authorization_code".into(),
                code: Some(code),
                redirect_uri: q.redirect_uri.clone(),
                client_id: Some("fixed".into()),
                client_secret: Some("fixed-secret".into()),
                code_verifier: Some(verifier),
                ..Default::default()
            })
            .unwrap();
        assert!(s.validate_bearer(&tokens.access_token));
        assert_eq!(tokens.token_type, "Bearer");
        assert!(!tokens.refresh_token.is_empty());
    }

    #[test]
    fn pkce_mismatch_rejected() {
        let s = server();
        let (_verifier, challenge) = pkce_pair();
        let q = AuthorizeQuery {
            response_type: Some("code".into()),
            client_id: Some("fixed".into()),
            redirect_uri: Some("http://127.0.0.1:9/cb".into()),
            state: None,
            scope: None,
            code_challenge: Some(challenge),
            code_challenge_method: Some("S256".into()),
            resource: None,
        };
        let rid = s.begin_authorize(&q).unwrap();
        let loc = s.approve(&rid, "secret-pin").unwrap();
        let code = loc
            .split("code=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .to_string();
        let err = s
            .exchange_token(TokenRequest {
                grant_type: "authorization_code".into(),
                code: Some(code),
                redirect_uri: q.redirect_uri.clone(),
                client_id: Some("fixed".into()),
                client_secret: Some("fixed-secret".into()),
                code_verifier: Some("b".repeat(64)),
                ..Default::default()
            })
            .unwrap_err();
        assert!(matches!(err, OauthError::InvalidGrant(_)));
    }

    #[test]
    fn wrong_password_denied() {
        let s = server();
        let ( _v, challenge) = pkce_pair();
        let q = AuthorizeQuery {
            response_type: Some("code".into()),
            client_id: Some("fixed".into()),
            redirect_uri: Some("http://127.0.0.1:9/cb".into()),
            state: None,
            scope: None,
            code_challenge: Some(challenge),
            code_challenge_method: Some("S256".into()),
            resource: None,
        };
        let rid = s.begin_authorize(&q).unwrap();
        let err = s.approve(&rid, "nope").unwrap_err();
        assert!(matches!(err, OauthError::AccessDenied(_)));
    }

    #[test]
    fn dcr_then_authorize() {
        let s = server();
        let reg = s
            .register_client(RegisterRequest {
                redirect_uris: vec!["https://gemini.google.com/oauth".into()],
                client_name: Some("Gemini".into()),
                token_endpoint_auth_method: Some("none".into()),
                grant_types: vec![],
                response_types: vec![],
            })
            .unwrap();
        assert!(reg.client_secret.is_none());
        let (_v, challenge) = pkce_pair();
        let q = AuthorizeQuery {
            response_type: Some("code".into()),
            client_id: Some(reg.client_id),
            redirect_uri: Some("https://gemini.google.com/oauth".into()),
            state: Some("s".into()),
            scope: None,
            code_challenge: Some(challenge),
            code_challenge_method: Some("S256".into()),
            resource: None,
        };
        assert!(s.begin_authorize(&q).is_ok());
    }

    #[test]
    fn rejects_javascript_redirect() {
        assert!(!is_allowed_redirect("javascript:alert(1)"));
        assert!(!is_allowed_redirect("http://evil.example/cb"));
        assert!(is_allowed_redirect("https://chatgpt.com/cb"));
        assert!(is_allowed_redirect("http://127.0.0.1:1234/cb"));
    }

    #[test]
    fn metadata_contains_required_fields() {
        let s = server();
        let as_meta = s.authorization_server_metadata("https://host.example");
        assert_eq!(as_meta["issuer"], "https://host.example");
        assert_eq!(
            as_meta["authorization_endpoint"],
            "https://host.example/oauth/authorize"
        );
        assert_eq!(as_meta["token_endpoint"], "https://host.example/oauth/token");
        assert_eq!(as_meta["code_challenge_methods_supported"][0], "S256");
        let pr = s.protected_resource_metadata("https://host.example");
        assert_eq!(pr["resource"], "https://host.example/mcp");
        assert_eq!(pr["scopes_supported"][0], "mcp:read");
    }
}
