// src/oauth/server.rs
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::oauth::crypto::{
    ct_eq, normalize_scope, pkce_challenge, random_token, redirect_error, redirect_success,
};
use crate::oauth::guard::BruteForceGuard;
use crate::oauth::storage::{
    is_allowed_redirect, unix_now, AuthCode, IssuedToken, PendingAuth, RegisteredClient,
};
use crate::oauth::{
    AuthorizeQuery, OauthError, OauthSettings, RegisterRequest, RegisterResponse, TokenRequest,
    TokenResponse, SCOPES_SUPPORTED,
};
use crate::storage::Storage;

const ACCESS_TTL: u64 = 3600;
const REFRESH_TTL: u64 = 30 * 24 * 3600;
const CODE_TTL: u64 = 600;
const PENDING_TTL: u64 = 600;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectedClientInfo {
    pub client_id: String,
    pub client_name: String,
    pub has_active_token: bool,
}

#[derive(Clone)]
pub struct OauthServer {
    db: Arc<Storage>, // <--- 原先的 Memory 变成了纯净的 SQLite 连接
    guard: Arc<Mutex<BruteForceGuard>>,
    admin_password: String,
    password_generated: bool,
    static_token: Option<String>,
    require_auth: bool,
}

impl OauthServer {
    pub fn new(settings: OauthSettings, db: Arc<Storage>) -> Self {
        // 自动注册配置中给定的静态 Client (如果存在的话)
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
            let _ = db.insert_oauth_client(&RegisteredClient {
                client_id: id,
                client_secret: secret.clone(),
                client_name: "AgentBridge Local Config".into(),
                redirect_uris: Vec::new(),
                token_endpoint_auth_method: if secret.is_some() {
                    "client_secret_post".into()
                } else {
                    "none".into()
                },
            });
        }

        Self {
            db,
            guard: Arc::new(Mutex::new(BruteForceGuard::new())),
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
    pub fn admin_password_value(&self) -> &str {
        &self.admin_password
    }
    pub fn has_static_token(&self) -> bool {
        self.static_token.is_some()
    }

    pub fn authorization_server_metadata(&self, issuer: &str) -> serde_json::Value {
        let issuer = issuer.trim_end_matches('/');
        serde_json::json!({
            "issuer": issuer, "authorization_endpoint": format!("{issuer}/oauth/authorize"),
            "token_endpoint": format!("{issuer}/oauth/token"), "registration_endpoint": format!("{issuer}/oauth/register"),
            "revocation_endpoint": format!("{issuer}/oauth/revoke"), "scopes_supported": SCOPES_SUPPORTED,
            "response_types_supported": ["code"], "grant_types_supported": ["authorization_code", "refresh_token"],
            "code_challenge_methods_supported": ["S256"], "token_endpoint_auth_methods_supported": ["none", "client_secret_post"],
        })
    }

    pub fn protected_resource_metadata(&self, issuer: &str) -> serde_json::Value {
        serde_json::json!({ "resource": format!("{}/mcp", issuer.trim_end_matches('/')), "authorization_servers": [issuer.trim_end_matches('/')], "bearer_methods_supported": ["header"], "scopes_supported": SCOPES_SUPPORTED })
    }

    pub fn www_authenticate(&self, issuer: &str) -> axum::http::HeaderValue {
        let val = format!(
            r#"Bearer realm="mcp", resource_metadata="{}/.well-known/oauth-protected-resource""#,
            issuer.trim_end_matches('/')
        );
        axum::http::HeaderValue::from_str(&val)
            .unwrap_or_else(|_| axum::http::HeaderValue::from_static("Bearer"))
    }

    pub fn validate_bearer(&self, token: &str) -> bool {
        if token.is_empty() {
            return false;
        }
        if self
            .static_token
            .as_ref()
            .is_some_and(|st| ct_eq(token, st))
        {
            return true;
        }
        self.db.is_access_token_valid(token, unix_now())
    }

    pub fn get_client_name(&self, client_id: &str) -> String {
        self.db
            .get_oauth_client(client_id)
            .map(|c| c.client_name)
            .unwrap_or_else(|| "MCP Client".into())
    }

    pub fn list_connected_clients(&self) -> Vec<ConnectedClientInfo> {
        self.db.get_active_clients(unix_now())
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
        if req.code_challenge_method.as_deref().unwrap_or("") != "S256" {
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

        let client = self
            .db
            .get_oauth_client(client_id)
            .ok_or_else(|| OauthError::InvalidClient("unknown client_id".into()))?;
        if !client.redirect_allowed(redirect_uri) {
            return Err(OauthError::InvalidRequest(
                "redirect_uri is not registered".into(),
            ));
        }

        let request_id = random_token("abq_");
        self.db
            .insert_oauth_pending(
                &request_id,
                &PendingAuth {
                    client_id: client.client_id,
                    redirect_uri: redirect_uri.to_string(),
                    state: req.state.clone(),
                    code_challenge: challenge.to_string(),
                    scope: normalize_scope(req.scope.as_deref()),
                    resource: req.resource.clone(),
                    expires_at: unix_now() + PENDING_TTL,
                },
            )
            .map_err(|e| OauthError::ServerError(e.to_string()))?;

        Ok(request_id)
    }

    pub fn approve(&self, request_id: &str, password: &str) -> Result<String, OauthError> {
        let is_correct =
            ct_eq(password.trim(), &self.admin_password) && !self.admin_password.is_empty();
        self.guard.lock().unwrap().check(is_correct)?;

        let pending = self
            .db
            .take_oauth_pending(request_id)
            .ok_or_else(|| OauthError::InvalidRequest("authorization request expired".into()))?;
        if pending.expires_at <= unix_now() {
            return Err(OauthError::InvalidRequest(
                "authorization request expired".into(),
            ));
        }

        let code = random_token("abc_");
        let redirect = redirect_success(&pending.redirect_uri, &code, pending.state.as_deref());

        self.db
            .insert_oauth_code(
                &code,
                &AuthCode {
                    client_id: pending.client_id,
                    redirect_uri: pending.redirect_uri,
                    code_challenge: pending.code_challenge,
                    scope: pending.scope,
                    resource: pending.resource,
                    expires_at: unix_now() + CODE_TTL,
                },
            )
            .map_err(|e| OauthError::ServerError(e.to_string()))?;

        Ok(redirect)
    }

    pub fn deny(&self, request_id: &str) -> Result<String, OauthError> {
        let pending = self
            .db
            .take_oauth_pending(request_id)
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

        self.db
            .insert_oauth_client(&RegisteredClient {
                client_id: client_id.clone(),
                client_secret: secret.clone(),
                client_name: client_name.clone(),
                redirect_uris: body.redirect_uris.clone(),
                token_endpoint_auth_method: method.clone(),
            })
            .map_err(|e| OauthError::ServerError(e.to_string()))?;

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
            "authorization_code" => {
                let code = req
                    .code
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| OauthError::InvalidRequest("code required".into()))?;
                let verifier = req
                    .code_verifier
                    .as_deref()
                    .filter(|s| (43..=128).contains(&s.len()))
                    .ok_or_else(|| OauthError::InvalidRequest("verifier required".into()))?;
                let redirect_uri = req
                    .redirect_uri
                    .as_deref()
                    .ok_or_else(|| OauthError::InvalidRequest("redirect_uri required".into()))?;
                let client = self.authenticate_client(&req)?;

                let issued = self
                    .db
                    .take_oauth_code(code)
                    .ok_or_else(|| OauthError::InvalidGrant("invalid code".into()))?;
                if issued.expires_at <= unix_now() {
                    return Err(OauthError::InvalidGrant("code expired".into()));
                }
                if issued.client_id != client.client_id {
                    return Err(OauthError::InvalidGrant("client mismatch".into()));
                }
                if issued.redirect_uri.trim_end_matches('/') != redirect_uri.trim_end_matches('/') {
                    return Err(OauthError::InvalidGrant("uri mismatch".into()));
                }
                if pkce_challenge(verifier) != issued.code_challenge {
                    return Err(OauthError::InvalidGrant("PKCE failed".into()));
                }

                Ok(self.issue_tokens(&client.client_id, &issued.scope))
            }
            "refresh_token" => {
                let refresh = req
                    .refresh_token
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| OauthError::InvalidRequest("refresh_token required".into()))?;
                let client = self.authenticate_client(&req)?;
                let issued = self
                    .db
                    .take_refresh_token(refresh)
                    .ok_or_else(|| OauthError::InvalidGrant("invalid refresh".into()))?;
                if issued.expires_at <= unix_now() {
                    return Err(OauthError::InvalidGrant("refresh expired".into()));
                }
                if issued.client_id != client.client_id {
                    return Err(OauthError::InvalidGrant("client mismatch".into()));
                }

                Ok(self.issue_tokens(&client.client_id, &issued.scope))
            }
            other => Err(OauthError::InvalidRequest(format!(
                "unsupported grant_type: {other}"
            ))),
        }
    }

    pub fn revoke(&self, token: &str) {
        self.db.revoke_token(token);
    }

    fn authenticate_client(&self, req: &TokenRequest) -> Result<RegisteredClient, OauthError> {
        let client_id = req
            .client_id
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| OauthError::InvalidClient("client_id required".into()))?;
        let client = self
            .db
            .get_oauth_client(client_id)
            .ok_or_else(|| OauthError::InvalidClient("unknown client_id".into()))?;
        if let Some(expected) = &client.client_secret {
            let provided = req.client_secret.as_deref().unwrap_or("");
            if !ct_eq(provided, expected) {
                return Err(OauthError::InvalidClient("invalid secret".into()));
            }
        }
        Ok(client)
    }

    fn issue_tokens(&self, client_id: &str, scope: &str) -> TokenResponse {
        let access = random_token("abt_");
        let refresh = random_token("abr_");
        let now = unix_now();
        let access_expires = now + ACCESS_TTL;
        let refresh_expires = now + REFRESH_TTL;

        let t = IssuedToken {
            client_id: client_id.to_string(),
            scope: scope.to_string(),
            expires_at: access_expires,
        };
        let _ = self.db.issue_tokens(&access, &refresh, &t, refresh_expires);

        TokenResponse {
            access_token: access,
            token_type: "Bearer".into(),
            expires_in: ACCESS_TTL,
            refresh_token: refresh,
            scope: scope.to_string(),
        }
    }

    fn redirect_or(&self, req: &AuthorizeQuery, error: &str, desc: &str) -> OauthError {
        if let (Some(uri), Some(cid)) = (req.redirect_uri.as_deref(), req.client_id.as_deref()) {
            if is_allowed_redirect(uri)
                && self
                    .db
                    .get_oauth_client(cid)
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
        OauthError::InvalidRequest(desc.to_string())
    }
}
