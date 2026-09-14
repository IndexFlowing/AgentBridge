use axum::body::Bytes;
use axum::extract::{Query, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

use crate::oauth::server::OauthServer;
use crate::oauth::views::{consent_page, consent_page_error, html_escape, idle_page};
use crate::oauth::{
    ApproveForm, AuthHttpState, AuthorizeQuery, OauthError, RegisterRequest, TokenRequest,
};

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
        .route(
            "/.well-known/openid-configuration",
            get(authorization_server),
        )
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
    let valid = token.is_some_and(|t| state.oauth.validate_bearer(t));
    eprintln!(
        "[OAuth-MCP] 拦截检查: path={}, token={:?}, valid={}",
        req.uri().path(),
        token.map(|t| &t[..t.len().min(12)]),
        valid
    );
    if valid {
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
    eprintln!(
        "[OAuth GET /authorize] 收到授权页面请求: client_id={:?}, redirect_uri={:?}",
        query.client_id, query.redirect_uri
    );
    if query.client_id.is_none() && query.redirect_uri.is_none() {
        return Html(idle_page()).into_response();
    }
    match state.oauth.begin_authorize(&query) {
        Ok(request_id) => {
            eprintln!("[OAuth GET /authorize] 成功生成授权会话 rid={}", request_id);
            let client = query
                .client_id
                .as_deref()
                .map(|id| state.oauth.get_client_name(id))
                .unwrap_or_else(|| "ChatGPT".to_string());
            Html(consent_page(&request_id, &client, query.scope.as_deref())).into_response()
        }
        Err(OauthError::Redirect(url)) => {
            eprintln!("[OAuth GET /authorize] 异常重定向: {}", url);
            Redirect::to(&url).into_response()
        }
        Err(err) => {
            eprintln!("[OAuth GET /authorize] 授权初始化失败: {}", err);
            oauth_html_error(StatusCode::BAD_REQUEST, &err.to_string())
        }
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

    eprintln!(
        "[OAuth POST /authorize] 收到审批提交, request_id={}",
        form.request_id
    );

    match state.oauth.approve(&form.request_id, &form.password) {
        Ok(url) => {
            eprintln!(
                "[OAuth POST /authorize] ✅ 密码正确，重定向回客户端: {}",
                url
            );
            Redirect::to(&url).into_response()
        }
        Err(OauthError::AccessDenied(_)) => {
            eprintln!("[OAuth POST /authorize] ❌ 密码错误！");
            Html(consent_page_error(
                &form.request_id,
                "MCP client",
                "Incorrect password. Try again.",
            ))
            .into_response()
        }
        Err(OauthError::Locked) => {
            eprintln!("[OAuth POST /authorize] ❌ 密码错误过多被锁定！");
            oauth_html_error(
                StatusCode::TOO_MANY_REQUESTS,
                &OauthError::Locked.to_string(),
            )
        }
        Err(err) => {
            eprintln!("[OAuth POST /authorize] ❌ 审批失败: {}", err);
            oauth_html_error(StatusCode::BAD_REQUEST, &err.to_string())
        }
    }
}

async fn token_post(
    State(state): State<AuthHttpState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    eprintln!(
        "[OAuth POST /token] 收到换取 Token 请求, body 长度={}",
        body.len()
    );
    let mut req = match parse_token_body(&headers, &body) {
        Ok(r) => r,
        Err(err) => {
            eprintln!("[OAuth POST /token] ❌ 解析请求体失败: {}", err);
            return token_error(StatusCode::BAD_REQUEST, "invalid_request", &err);
        }
    };
    apply_basic_auth(&headers, &mut req);
    eprintln!(
        "[OAuth POST /token] 收到参数: grant_type={}, client_id={:?}, redirect_uri={:?}, code={}, refresh_token={}",
        req.grant_type,
        req.client_id,
        req.redirect_uri,
        req.code.is_some(),
        req.refresh_token.is_some()
    );

    match state.oauth.exchange_token(req) {
        Ok(tokens) => {
            eprintln!("[OAuth POST /token] ✅ Token 换取成功");
            (
                StatusCode::OK,
                [
                    (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
                    (header::PRAGMA, HeaderValue::from_static("no-cache")),
                ],
                Json(tokens),
            )
                .into_response()
        }
        Err(err) => {
            eprintln!(
                "[OAuth POST /token] ❌ Token 换取被拒绝，核心原因: {:?}",
                err
            );
            match err {
                OauthError::InvalidClient(msg) => {
                    token_error(StatusCode::UNAUTHORIZED, "invalid_client", &msg)
                }
                OauthError::InvalidGrant(msg) => {
                    token_error(StatusCode::BAD_REQUEST, "invalid_grant", &msg)
                }
                _ => token_error(StatusCode::BAD_REQUEST, "invalid_request", &err.to_string()),
            }
        }
    }
}

async fn register_post(
    State(state): State<AuthHttpState>,
    Json(body): Json<RegisterRequest>,
) -> Response {
    eprintln!(
        "[OAuth POST /register] 收到动态注册: client_name={:?}, redirect_uris={:?}",
        body.client_name, body.redirect_uris
    );
    match state.oauth.register_client(body) {
        Ok(resp) => {
            eprintln!(
                "[OAuth POST /register] ✅ 注册成功: client_id={}",
                resp.client_id
            );
            (StatusCode::CREATED, Json(resp)).into_response()
        }
        Err(err) => {
            eprintln!("[OAuth POST /register] ❌ 注册失败: {}", err);
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_client_metadata",
                    "error_description": err.to_string(),
                })),
            )
                .into_response()
        }
    }
}

async fn revoke_post(
    State(state): State<AuthHttpState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
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
    let Some(value) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    else {
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
