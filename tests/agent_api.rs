//! Web Control Plane read surface for the Agent Runtime: the Agent root view
//! and the per-project AgentContext diagnostics endpoint.

use std::sync::Arc;

use agentbridge::config::Config;
use agentbridge::core::AppCore;
use agentbridge::oauth::{OauthServer, OauthSettings};
use agentbridge::server::build_router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tempfile::TempDir;
use tower::ServiceExt;

mod common;

fn app() -> axum::Router {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("README.md"), "ok\n").unwrap();
    let cfg = Arc::new(Config::new(dir.path().to_path_buf()));
    let storage = common::test_storage();
    let hub = Arc::new(common::hub_with(
        cfg.clone(),
        storage.clone(),
        vec![common::project_entry("default", dir.path().to_path_buf())],
    ));
    let oauth = Arc::new(OauthServer::new(
        OauthSettings {
            require_auth: false,
            admin_password: String::new(),
            password_generated: false,
            static_token: None,
            client_id: None,
            client_secret: None,
        },
        storage.clone(),
    ));
    std::mem::forget(dir);
    let core = Arc::new(AppCore::new(cfg, storage, hub));
    build_router(core, oauth, true)
}

async fn get_json(app: &axum::Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("host", "127.0.0.1:8030")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn agent_endpoint_exposes_manifest_rules_and_skills() {
    let app = app();
    let (status, body) = get_json(&app, "/api/agent").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.get("found").is_some(), "missing found: {body}");
    assert!(body.get("agent_dir").is_some(), "missing agent_dir: {body}");
    assert!(body.get("rules").is_some(), "missing rules: {body}");
    assert!(body.get("skills").is_some(), "missing skills: {body}");
}

#[tokio::test]
async fn agent_context_endpoint_resolves_selected_project() {
    let app = app();
    let (status, body) = get_json(&app, "/api/agent/context?project=default").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["project"], "default");
    assert!(body.get("workspace").is_some(), "missing workspace: {body}");
    assert!(
        body.get("agent_root").is_some(),
        "missing agent_root: {body}"
    );
    assert!(body["rules"].is_array(), "rules not an array: {body}");
    assert!(body["skills"].is_array(), "skills not an array: {body}");
}

#[tokio::test]
async fn agent_context_endpoint_rejects_unknown_project() {
    let app = app();
    let (status, _) = get_json(&app, "/api/agent/context?project=does-not-exist").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
