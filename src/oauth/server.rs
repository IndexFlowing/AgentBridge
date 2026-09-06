//! OAuth 2.1 协议状态机与业务流程编排

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::http::HeaderValue;
use serde::{Deserialize, Serialize};

use crate::oauth::crypto::{
    ct_eq, normalize_scope, pkce_challenge, random_token, redirect_error, redirect_success,
};
use crate::oauth::guard::BruteForceGuard;
use crate::oauth::storage::{
    is_allowed_redirect, unix_now, AuthCode, IssuedToken, PendingAuth, RegisteredClient,
    TokenStorage,
};
use crate::oauth::{
    AuthorizeQuery, OauthError, OauthSettings, RegisterRequest, RegisterResponse, TokenRequest,
    TokenResponse, SCOPES_SUPPORTED,
};

const ACCESS_TTL: Duration = Duration::from_secs(3600);
const REFRESH_TTL: Duration = Duration::from_secs(30 * 24 * 3600);
const CODE_TTL: Duration = Duration::from_secs(600);
const PENDING_TTL: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectedClientInfo {
    pub client_id: String,
    pub client_name: String,
    pub has_active_token: bool,
}

#[derive(Clone)]
pub struct OauthServer {
    storage: Arc<Mutex<TokenStorage>>,
    guard: Arc<Mutex<BruteForceGuard>>,
    admin_password: String,
    password_generated: bool,
    static_token: Option<String>,
    require_auth: bool,
}

impl OauthServer {
    pub fn new(settings: OauthSettings) -> Self {
        let mut storage = TokenStorage::load();

        if let Some(id) = settings.client_id.as_ref().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
            let secret = settings.client_secret.as_ref().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
            storage.clients.insert(
                id.clone(),
                RegisteredClient {
                    client_id: id,
                    client_secret: secret,
                    client_name: "AgentBridge".into(),
                    redirect_uris: Vec::new(),
                    token_endpoint_auth_method: if settings.client_secret.is_some() { "client_secret_post".into() } else { "none".into() },
                },
            );
        }

        Self {
            storage: Arc::new(Mutex::new(storage)),
            guard: Arc::new(Mutex::new(BruteForceGuard::new())),
            admin_password: settings.admin_password,
            password_generated: settings.password_generated,
            static_token: settings.static_token.filter(|s| !s.is_empty()),
            require_auth: settings.require_auth,
        }
    }

    pub fn require_auth(&self) -> bool { self.require_auth }
    pub fn generated_password(&self) -> Option<&str> { if self.password_generated && self.require_auth { Some(self.admin_password.as_str()) } else { None } }
    pub fn has_admin_password(&self) -> bool { !self.admin_password.is_empty() }
    pub fn admin_password_value(&self) -> &str { &self.admin_password }
    pub fn has_static_token(&self) -> bool { self.static_token.is_some() }

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
        let value = format!(r#"Bearer realm="mcp", resource_metadata="{issuer}/.well-known/oauth-protected-resource""#);
        HeaderValue::from_str(&value).unwrap_or_else(|_| HeaderValue::from_static("Bearer"))
    }

    pub fn validate_bearer(&self, token: &str) -> bool {
        if token.is_empty() { return false; }
        if let Some(static_token) = &self.static_token {
            if ct_eq(token, static_token) { return true; }
        }
        let mut storage = self.storage.lock().unwrap();
        storage.cleanup();
        let now = unix_now();
        storage.access.get(token).is_some_and(|t| t.expires_at > now)
    }

    pub fn get_client_name(&self, client_id: &str) -> String {
        let storage = self.storage.lock().unwrap();
        storage
            .clients
            .get(client_id)
            .map(|c| c.client_name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "ChatGPT".to_string())
    }

    pub fn list_connected_clients(&self) -> Vec<ConnectedClientInfo> {
        let storage = self.storage.lock().unwrap();
        let now = unix_now();
        let active_ids: HashSet<String> = storage.access.values().filter(|t| t.expires_at > now).map(|t| t.client_id.clone()).collect();
        active_ids.into_iter().filter_map(|id| {
            storage.clients.get(&id).map(|c| ConnectedClientInfo {
                client_id: c.client_id.clone(),
                client_name: c.client_name.clone(),
                has_active_token: true,
            })
        }).collect()
    }

    pub fn begin_authorize(&self, req: &AuthorizeQuery) -> Result<String, OauthError> {
        if req.response_type.as_deref().unwrap_or("") != "code" {
            return Err(self.redirect_or(req, "unsupported_response_type", "response_type must be code"));
        }
        let client_id = req.client_id.as_deref().map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| OauthError::InvalidRequest("client_id is required".into()))?;
        let redirect_uri = req.redirect_uri.as_deref().map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| OauthError::InvalidRequest("redirect_uri is required".into()))?;
        if !is_allowed_redirect(redirect_uri) {
            return Err(OauthError::InvalidRequest("redirect_uri must be https or loopback http".into()));
        }
        if req.code_challenge_method.as_deref().unwrap_or("") != "S256" {
            return Err(self.redirect_or(req, "invalid_request", "code_challenge_method must be S256"));
        }
        let challenge = req.code_challenge.as_deref().map(str::trim).filter(|s| s.len() >= 43).ok_or_else(|| self.redirect_or(req, "invalid_request", "code_challenge is required"))?;

        let mut storage = self.storage.lock().unwrap();
        storage.cleanup();
        let client = storage.clients.get(client_id).cloned().ok_or_else(|| OauthError::InvalidClient("unknown client_id".into()))?;
        if !client.redirect_allowed(redirect_uri) {
            return Err(OauthError::InvalidRequest("redirect_uri is not registered for this client".into()));
        }

        let request_id = random_token("abq_");
        storage.pending.insert(
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
        // 1. 防爆破守卫校验
        let is_correct = ct_eq(password.trim(), &self.admin_password) && !self.admin_password.is_empty();
        self.guard.lock().unwrap().check(is_correct)?;

        // 2. 仓储层读取与兑换
        let mut storage = self.storage.lock().unwrap();
        storage.cleanup();
        let pending = storage.pending.remove(request_id).ok_or_else(|| OauthError::InvalidRequest("authorization request expired".into()))?;
        if pending.expires_at <= Instant::now() {
            return Err(OauthError::InvalidRequest("authorization request expired".into()));
        }
        let code = random_token("abc_");
        let redirect = redirect_success(&pending.redirect_uri, &code, pending.state.as_deref());
        storage.codes.insert(
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
        let mut storage = self.storage.lock().unwrap();
        let pending = storage.pending.remove(request_id).ok_or_else(|| OauthError::InvalidRequest("authorization request expired".into()))?;
        Ok(redirect_error(&pending.redirect_uri, "access_denied", "The user denied the request", pending.state.as_deref()))
    }

    pub fn register_client(&self, body: RegisterRequest) -> Result<RegisterResponse, OauthError> {
        if body.redirect_uris.is_empty() { return Err(OauthError::InvalidRequest("redirect_uris is required".into())); }
        for uri in &body.redirect_uris {
            if !is_allowed_redirect(uri) { return Err(OauthError::InvalidRequest(format!("redirect_uri not allowed: {uri}"))); }
        }
        let client_id = random_token("abcid_");
        let method = body.token_endpoint_auth_method.as_deref().unwrap_or("none").to_string();
        let secret = if method == "none" { None } else { Some(random_token("abcs_")) };
        let issued_at = unix_now();
        let client_name = body.client_name.clone().unwrap_or_else(|| "MCP Client".into());
        let client = RegisteredClient {
            client_id: client_id.clone(),
            client_secret: secret.clone(),
            client_name: client_name.clone(),
            redirect_uris: body.redirect_uris.clone(),
            token_endpoint_auth_method: method.clone(),
        };
        {
            let mut storage = self.storage.lock().unwrap();
            storage.clients.insert(client_id.clone(), client);
            storage.save_clients();
        }
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
            other => Err(OauthError::InvalidRequest(format!("unsupported grant_type: {other}"))),
        }
    }

    pub fn revoke(&self, token: &str) {
        let mut storage = self.storage.lock().unwrap();
        storage.access.remove(token);
        storage.refresh.remove(token);
        storage.save_tokens();
    }

    fn exchange_code(&self, req: TokenRequest) -> Result<TokenResponse, OauthError> {
        let code = req.code.as_deref().filter(|s| !s.is_empty()).ok_or_else(|| OauthError::InvalidRequest("code is required".into()))?;
        let verifier = req.code_verifier.as_deref().filter(|s| (43..=128).contains(&s.len())).ok_or_else(|| OauthError::InvalidRequest("code_verifier is required".into()))?;
        let redirect_uri = req.redirect_uri.as_deref().ok_or_else(|| OauthError::InvalidRequest("redirect_uri is required".into()))?;

        let mut storage = self.storage.lock().unwrap();
        storage.cleanup();
        let client = self.authenticate_client(&storage, &req)?;
        let issued = storage.codes.remove(code).ok_or_else(|| OauthError::InvalidGrant("authorization code is invalid".into()))?;
        
        if issued.expires_at <= Instant::now() { 
            eprintln!("[OAuth] ❌ Code 已过期");
            return Err(OauthError::InvalidGrant("authorization code expired".into())); 
        }
        if issued.client_id != client.client_id { 
            eprintln!("[OAuth] ❌ Client ID 不匹配: issued={}, req={}", issued.client_id, client.client_id);
            return Err(OauthError::InvalidGrant("code was issued to another client".into())); 
        }
        
        // 👈 宽松比对末尾斜杠
        if issued.redirect_uri.trim_end_matches('/') != redirect_uri.trim_end_matches('/') { 
            eprintln!("[OAuth] ❌ 回调地址不匹配: 记录的是 '{}', 收到的是 '{}'", issued.redirect_uri, redirect_uri);
            return Err(OauthError::InvalidGrant("redirect_uri mismatch".into())); 
        }
        
        if pkce_challenge(verifier) != issued.code_challenge { 
            eprintln!("[OAuth] ❌ PKCE 校验失败！");
            return Err(OauthError::InvalidGrant("PKCE verification failed".into())); 
        }

        let tokens = issue_tokens(&mut storage, &client.client_id, &issued.scope);
        storage.save_tokens();
        Ok(tokens)
    }

    fn exchange_refresh(&self, req: TokenRequest) -> Result<TokenResponse, OauthError> {
        let refresh = req.refresh_token.as_deref().filter(|s| !s.is_empty()).ok_or_else(|| OauthError::InvalidRequest("refresh_token is required".into()))?;
        let mut storage = self.storage.lock().unwrap();
        storage.cleanup();
        let client = self.authenticate_client(&storage, &req)?;
        let issued = storage.refresh.remove(refresh).ok_or_else(|| OauthError::InvalidGrant("refresh_token is invalid".into()))?;
        let now = unix_now();
        if issued.expires_at <= now { return Err(OauthError::InvalidGrant("refresh_token expired".into())); }
        if issued.client_id != client.client_id { return Err(OauthError::InvalidGrant("refresh_token was issued to another client".into())); }

        let tokens = issue_tokens(&mut storage, &client.client_id, &issued.scope);
        storage.save_tokens();
        Ok(tokens)
    }

    fn authenticate_client(&self, storage: &TokenStorage, req: &TokenRequest) -> Result<RegisteredClient, OauthError> {
        let client_id = req.client_id.as_deref().filter(|s| !s.is_empty()).ok_or_else(|| OauthError::InvalidClient("client_id is required".into()))?;
        let client = storage.clients.get(client_id).cloned().ok_or_else(|| OauthError::InvalidClient("unknown client_id".into()))?;
        if let Some(expected) = &client.client_secret {
            let provided = req.client_secret.as_deref().unwrap_or("");
            if !ct_eq(provided, expected) { return Err(OauthError::InvalidClient("invalid client_secret".into())); }
        }
        Ok(client)
    }

    fn redirect_or(&self, req: &AuthorizeQuery, error: &str, desc: &str) -> OauthError {
        if let (Some(uri), Some(client_id)) = (req.redirect_uri.as_deref(), req.client_id.as_deref()) {
            if is_allowed_redirect(uri) {
                let storage = self.storage.lock().unwrap();
                if storage.clients.get(client_id).is_some_and(|c| c.redirect_allowed(uri)) {
                    return OauthError::Redirect(redirect_error(uri, error, desc, req.state.as_deref()));
                }
            }
        }
        OauthError::InvalidRequest(desc.to_string())
    }
}

fn issue_tokens(storage: &mut TokenStorage, client_id: &str, scope: &str) -> TokenResponse {
    let access = random_token("abt_");
    let refresh = random_token("abr_");
    let now = unix_now();
    storage.access.insert(
        access.clone(),
        IssuedToken {
            client_id: client_id.to_string(),
            scope: scope.to_string(),
            expires_at: now + ACCESS_TTL.as_secs(),
        },
    );
    storage.refresh.insert(
        refresh.clone(),
        IssuedToken {
            client_id: client_id.to_string(),
            scope: scope.to_string(),
            expires_at: now + REFRESH_TTL.as_secs(),
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