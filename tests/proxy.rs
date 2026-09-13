//! Proxy Secret preservation and runtime hot-reload tests.

use axum::extract::State;
use axum::Json;
use std::sync::Arc;
use tempfile::TempDir;

use agentbridge::api::system::save_proxy;
use agentbridge::api::ApiState;
use agentbridge::config::{Config, ExecutorDefinition, ProxyKind};
use agentbridge::executor::ExecutorRegistry;
use agentbridge::models::ProxyInput;
use agentbridge::oauth::{OauthServer, OauthSettings};

mod common;

fn test_api_state(workspace: &std::path::Path) -> ApiState {
    let config = Arc::new(Config::new(workspace.to_path_buf()));
    let storage = common::test_storage();
    let hub = Arc::new(common::hub_with(
        config.clone(),
        storage.clone(),
        vec![common::project_entry("default", workspace.to_path_buf())],
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
    ApiState {
        config,
        hub,
        oauth,
        storage,
    }
}

/// Test 1: 修改 host/port 等普通字段时，若不传凭据或传空白，原有 username/password 必须完整保留
#[tokio::test]
async fn test_save_proxy_preserves_secret_when_omitted() {
    let dir = TempDir::new().unwrap();
    let state = test_api_state(dir.path());

    // 1. 首次保存，设置完整凭据
    let initial = ProxyInput {
        enabled: true,
        kind: ProxyKind::Http,
        host: "old-host".into(),
        port: 7890,
        username: Some("user".into()),
        password: Some("secret".into()),
    };
    let _ = save_proxy(State(state.clone()), Json(initial))
        .await
        .unwrap();

    // 2. 修改 host/port，username/password 传入 None
    let update_none = ProxyInput {
        enabled: true,
        kind: ProxyKind::Http,
        host: "new-host".into(),
        port: 7891,
        username: None,
        password: None,
    };
    let res = save_proxy(State(state.clone()), Json(update_none))
        .await
        .unwrap();
    assert_eq!(res.host, "new-host");
    assert_eq!(res.port, 7891);
    assert!(res.username_configured);
    assert!(res.password_configured);

    // 检查数据库底层真实数据
    let saved = state
        .storage
        .load_proxies()
        .unwrap()
        .into_iter()
        .find(|p| p.is_default)
        .unwrap();
    assert_eq!(saved.host, "new-host");
    assert_eq!(saved.port, 7891);
    assert_eq!(saved.username.as_deref(), Some("user"));
    assert_eq!(saved.password.as_deref(), Some("secret"));

    // 3. 传入空白字符串，空值 ≠ 清除，依然保留凭据
    let update_blank = ProxyInput {
        enabled: true,
        kind: ProxyKind::Http,
        host: "blank-host".into(),
        port: 7892,
        username: Some("   ".into()),
        password: Some("".into()),
    };
    let _ = save_proxy(State(state.clone()), Json(update_blank))
        .await
        .unwrap();
    let saved_after_blank = state
        .storage
        .load_proxies()
        .unwrap()
        .into_iter()
        .find(|p| p.is_default)
        .unwrap();
    assert_eq!(saved_after_blank.host, "blank-host");
    assert_eq!(saved_after_blank.username.as_deref(), Some("user"));
    assert_eq!(saved_after_blank.password.as_deref(), Some("secret"));
}

/// Test 2: 显式提供新凭据时，必须正常更新为新凭据
#[tokio::test]
async fn test_save_proxy_updates_secret_when_provided() {
    let dir = TempDir::new().unwrap();
    let state = test_api_state(dir.path());

    // 1. 初始配置
    let initial = ProxyInput {
        enabled: true,
        kind: ProxyKind::Http,
        host: "host".into(),
        port: 7890,
        username: Some("old-user".into()),
        password: Some("old-password".into()),
    };
    let _ = save_proxy(State(state.clone()), Json(initial))
        .await
        .unwrap();

    // 2. 提交新凭据
    let update = ProxyInput {
        enabled: true,
        kind: ProxyKind::Http,
        host: "host".into(),
        port: 7890,
        username: Some("new-user".into()),
        password: Some("new-password".into()),
    };
    let _ = save_proxy(State(state.clone()), Json(update))
        .await
        .unwrap();

    let saved = state
        .storage
        .load_proxies()
        .unwrap()
        .into_iter()
        .find(|p| p.is_default)
        .unwrap();
    assert_eq!(saved.username.as_deref(), Some("new-user"));
    assert_eq!(saved.password.as_deref(), Some("new-password"));
}

/// Test 3: save_proxy 后，reload 使得 Runtime 能立即解析到新 Proxy 且凭据完整保留
#[tokio::test]
async fn test_runtime_uses_updated_proxy_after_reload() {
    let dir = TempDir::new().unwrap();
    let state = test_api_state(dir.path());

    // 注册一个指定使用 default 代理的执行器
    let exec_def = ExecutorDefinition {
        id: "test-exec".into(),
        name: "Test Executor".into(),
        display_name: "Test Executor".into(),
        kind: "opencode".into(),
        command: "opencode".into(),
        executable: None,
        working_directory: None,
        proxy_id: Some("default".into()),
        enabled: true,
    };
    state.storage.upsert_executor(exec_def).unwrap();
    state.hub.reload_executors().unwrap();

    // 1. 保存初始代理（带凭据）
    let initial = ProxyInput {
        enabled: true,
        kind: ProxyKind::Socks5,
        host: "proxy1.local".into(),
        port: 1080,
        username: Some("proxy-user".into()),
        password: Some("proxy-pass".into()),
    };
    let _ = save_proxy(State(state.clone()), Json(initial))
        .await
        .unwrap();

    // 2. 仅修改 host/port，凭据留空
    let update = ProxyInput {
        enabled: true,
        kind: ProxyKind::Socks5,
        host: "proxy2.local".into(),
        port: 1081,
        username: None,
        password: None,
    };
    let _ = save_proxy(State(state.clone()), Json(update))
        .await
        .unwrap();

    // 3. 验证 Runtime 内存注册表中立即能够解析到最新代理配置与完整凭据
    let definitions = state.storage.load_executors().unwrap();
    let proxies = state.storage.load_proxies().unwrap();
    let registry = ExecutorRegistry::from_config(&state.config, &definitions, &proxies).unwrap();

    let resolved = registry
        .resolve_proxy_for("test-exec")
        .expect("proxy should resolve");
    assert_eq!(resolved.host, "proxy2.local");
    assert_eq!(resolved.port, 1081);
    assert_eq!(resolved.username.as_deref(), Some("proxy-user"));
    assert_eq!(resolved.password.as_deref(), Some("proxy-pass"));
    let url = resolved.url().unwrap();
    assert!(url.starts_with("socks5h://"));
    assert!(url.ends_with("@proxy2.local:1081"));
    assert_eq!(
        url,
        "socks5h://%70%72%6F%78%79%2D%75%73%65%72:%70%72%6F%78%79%2D%70%61%73%73@proxy2.local:1081"
    );
}
