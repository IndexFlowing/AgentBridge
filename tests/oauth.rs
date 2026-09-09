//! OAuth 2.1 HTTP surface: metadata, 401 challenge, token, bearer access.

use std::sync::Arc;

use agentbridge::config::Config;
use agentbridge::oauth::{
    generate_admin_password, OauthServer, OauthSettings, RegisterRequest, TokenRequest,
};
use agentbridge::projects::ProjectHub;
use agentbridge::server::{build_oauth, build_router, ServeOptions};
use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use tempfile::TempDir;
use tower::ServiceExt;

fn app(require_auth: bool, password: &str, static_token: Option<&str>) -> axum::Router {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("README.md"), "ok\n").unwrap();
    let mut cfg = Config::new(dir.path().to_path_buf());
    cfg.auth_token = static_token.map(ToOwned::to_owned);
    cfg.port = 8030;
    let cfg = Arc::new(cfg);
    let hub = Arc::new(ProjectHub::single(dir.path().to_path_buf(), cfg.clone()).unwrap());
    let oauth = Arc::new(OauthServer::new(OauthSettings {
        require_auth,
        admin_password: password.into(),
        password_generated: false,
        static_token: static_token.map(ToOwned::to_owned),
        client_id: None,
        client_secret: None,
    }));
    std::mem::forget(dir);
    build_router(cfg, hub, oauth, true)
}

async fn body_json(res: axum::response::Response) -> serde_json::Value {
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn mcp_without_bearer_returns_401_with_resource_metadata() {
    let app = app(true, "pin", None);
    let res = app
        .oneshot(
            Request::builder()
                .uri("/mcp")
                .header("host", "127.0.0.1:8030")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let www = res
        .headers()
        .get(header::WWW_AUTHENTICATE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(www.contains("Bearer"), "{www}");
    assert!(www.contains("realm=\"mcp\""), "{www}");
    assert!(
        www.contains("resource_metadata="),
        "missing resource_metadata: {www}"
    );
    assert!(
        www.contains("/.well-known/oauth-protected-resource"),
        "{www}"
    );
}

#[tokio::test]
async fn protected_resource_metadata() {
    let app = app(true, "pin", None);
    let res = app
        .oneshot(
            Request::builder()
                .uri("/.well-known/oauth-protected-resource")
                .header("host", "example.trycloudflare.com")
                .header("x-forwarded-proto", "https")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json = body_json(res).await;
    assert_eq!(json["resource"], "https://example.trycloudflare.com/mcp");
    assert_eq!(
        json["authorization_servers"][0],
        "https://example.trycloudflare.com"
    );
    assert_eq!(json["scopes_supported"][0], "mcp:read");
    assert_eq!(json["scopes_supported"][1], "mcp:write");
}

#[tokio::test]
async fn authorization_server_metadata() {
    let app = app(true, "pin", None);
    let res = app
        .oneshot(
            Request::builder()
                .uri("/.well-known/oauth-authorization-server")
                .header("host", "example.trycloudflare.com")
                .header("x-forwarded-proto", "https")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json = body_json(res).await;
    assert_eq!(json["issuer"], "https://example.trycloudflare.com");
    assert_eq!(
        json["authorization_endpoint"],
        "https://example.trycloudflare.com/oauth/authorize"
    );
    assert_eq!(
        json["token_endpoint"],
        "https://example.trycloudflare.com/oauth/token"
    );
    assert_eq!(json["response_types_supported"][0], "code");
    assert!(json["grant_types_supported"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "authorization_code"));
    assert!(json["grant_types_supported"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "refresh_token"));
    assert_eq!(json["code_challenge_methods_supported"][0], "S256");
}

#[tokio::test]
async fn static_bearer_token_unlocks_mcp() {
    let app = app(true, "pin", Some("fixed-token"));
    let denied = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/mcp")
                .header("host", "127.0.0.1:8030")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);

    let res = app
        .oneshot(
            Request::builder()
                .uri("/mcp")
                .header("host", "127.0.0.1:8030")
                .header(header::AUTHORIZATION, "Bearer fixed-token")
                .header(header::ACCEPT, "application/json, text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn no_auth_skips_401() {
    let app = app(false, "", None);
    let res = app
        .oneshot(
            Request::builder()
                .uri("/mcp")
                .header("host", "127.0.0.1:8030")
                .header(header::ACCEPT, "application/json, text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn oauth_code_flow_token_accesses_mcp() {
    let password = "test-pin";
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("README.md"), "ok\n").unwrap();
    let cfg = Arc::new(Config::new(dir.path().to_path_buf()));
    let hub = Arc::new(ProjectHub::single(dir.path().to_path_buf(), cfg.clone()).unwrap());
    let oauth = Arc::new(OauthServer::new(OauthSettings {
        require_auth: true,
        admin_password: password.into(),
        password_generated: false,
        static_token: None,
        client_id: None,
        client_secret: None,
    }));
    std::mem::forget(dir);

    let verifier = "a".repeat(64);
    let challenge = {
        use sha2::{Digest, Sha256};
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
    };

    let registered = oauth
        .register_client(RegisterRequest {
            redirect_uris: vec!["https://chatgpt.com/connector_platform_oauth_redirect".into()],
            client_name: Some("ChatGPT".into()),
            token_endpoint_auth_method: Some("none".into()),
            grant_types: vec![],
            response_types: vec![],
        })
        .unwrap();

    let request_id = oauth
        .begin_authorize(&agentbridge::oauth::AuthorizeQuery {
            response_type: Some("code".into()),
            client_id: Some(registered.client_id.clone()),
            redirect_uri: Some("https://chatgpt.com/connector_platform_oauth_redirect".into()),
            state: Some("s1".into()),
            scope: Some("mcp:read mcp:write".into()),
            code_challenge: Some(challenge),
            code_challenge_method: Some("S256".into()),
            resource: Some("https://host/mcp".into()),
        })
        .unwrap();
    let loc = oauth.approve(&request_id, password).unwrap();
    assert!(loc.contains("state=s1"));
    let code = loc
        .split("code=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap()
        .to_string();

    let tokens = oauth
        .exchange_token(TokenRequest {
            grant_type: "authorization_code".into(),
            code: Some(code),
            redirect_uri: Some("https://chatgpt.com/connector_platform_oauth_redirect".into()),
            client_id: Some(registered.client_id),
            code_verifier: Some(verifier),
            ..Default::default()
        })
        .unwrap();

    let app = build_router(cfg, hub, oauth, true);
    let res = app
        .oneshot(
            Request::builder()
                .uri("/mcp")
                .header("host", "127.0.0.1:8030")
                .header(
                    header::AUTHORIZATION,
                    format!("Bearer {}", tokens.access_token),
                )
                .header(header::ACCEPT, "application/json, text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn generated_password_is_typeable() {
    let pin = generate_admin_password();
    assert_eq!(pin.len(), 14);
    assert_eq!(pin.chars().filter(|c| *c == '-').count(), 2);
}

#[test]
fn serve_options_default_enables_oauth() {
    let dir = TempDir::new().unwrap();
    let cfg = Config::new(dir.path().to_path_buf());
    let options = ServeOptions {
        allow_any_host: false,
        no_auth: false,
        client_id: None,
        client_secret: None,
        admin_password: None,
    };
    let (oauth, require) = build_oauth(&cfg, &options);
    assert!(require);
    assert!(oauth.generated_password().is_some());
}