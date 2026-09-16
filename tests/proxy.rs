//! Proxy Secret preservation and runtime hot-reload tests.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use std::sync::Arc;
use tempfile::TempDir;

use agentbridge::api::proxies::{
    create_proxy, delete_proxy, list_proxies, set_proxy_enabled, update_proxy, verify_proxy,
    ProxyEnabledInput,
};
use agentbridge::api::system::save_proxy;
use agentbridge::api::ApiState;
use agentbridge::config::{Config, ExecutorDefinition, ProxyKind};
use agentbridge::executor::ExecutorRegistry;
use agentbridge::models::{ProxyInput, SaveProxyRequest};
use agentbridge::oauth::{OauthServer, OauthSettings};
use agentbridge::storage::proxies::ProxyDefinition;

mod common;

fn proxy_request(name: &str, kind: ProxyKind, host: &str, port: u16) -> SaveProxyRequest {
    SaveProxyRequest {
        id: None,
        name: name.into(),
        kind,
        host: host.into(),
        port,
        username: None,
        password: None,
        enabled: true,
        is_default: false,
        test_url: None,
    }
}

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
    let core = Arc::new(agentbridge::core::AppCore::new(config, storage, hub));
    ApiState { core, oauth }
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

/// Test 4: 多代理 CRUD 生命周期（首个自动成为默认，更新保留凭据，启用/禁用，删除）
#[tokio::test]
async fn proxy_crud_lifecycle() {
    let dir = TempDir::new().unwrap();
    let state = test_api_state(dir.path());

    let created = create_proxy(
        State(state.clone()),
        Json(proxy_request("A", ProxyKind::Http, "a.local", 8080)),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(created.len(), 1);
    assert!(created[0].is_default, "首个代理应自动成为默认");
    let id_a = created[0].id.clone();

    let created = create_proxy(
        State(state.clone()),
        Json(proxy_request("B", ProxyKind::Socks5, "b.local", 1080)),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(created.len(), 2);
    let id_b = created.iter().find(|p| p.name == "B").unwrap().id.clone();
    assert_ne!(id_a, id_b);

    let mut update = proxy_request("B2", ProxyKind::Socks5, "b2.local", 1081);
    update.username = Some("user".into());
    update.password = Some("pass".into());
    let updated = update_proxy(State(state.clone()), Path(id_b.clone()), Json(update))
        .await
        .unwrap()
        .0;
    let b = updated.iter().find(|p| p.id == id_b).unwrap();
    assert_eq!(b.name, "B2");
    assert_eq!(b.host, "b2.local");
    assert_eq!(b.port, 1081);
    assert!(b.username_configured && b.password_configured);

    let toggled = set_proxy_enabled(
        State(state.clone()),
        Path(id_b.clone()),
        Json(ProxyEnabledInput { enabled: false }),
    )
    .await
    .unwrap()
    .0;
    assert!(!toggled.iter().find(|p| p.id == id_b).unwrap().enabled);

    let remaining = delete_proxy(State(state.clone()), Path(id_b))
        .await
        .unwrap()
        .0;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, id_a);

    let listed = list_proxies(State(state.clone())).await.unwrap().0;
    assert_eq!(listed.len(), 1);
}

/// Test 5: 创建时校验输入（端口为 0 应被拒绝）
#[tokio::test]
async fn proxy_create_rejects_invalid_input() {
    let dir = TempDir::new().unwrap();
    let state = test_api_state(dir.path());
    let err = create_proxy(
        State(state.clone()),
        Json(proxy_request("Bad", ProxyKind::Http, "bad.local", 0)),
    )
    .await
    .unwrap_err();
    assert_eq!(err.0, StatusCode::BAD_REQUEST);
}

/// Test 6: 删除仍被执行器引用的代理必须被阻止，避免悬空绑定
#[tokio::test]
async fn delete_proxy_blocked_while_referenced_by_executor() {
    let dir = TempDir::new().unwrap();
    let state = test_api_state(dir.path());

    let created = create_proxy(
        State(state.clone()),
        Json(proxy_request("Bound", ProxyKind::Http, "bound.local", 8080)),
    )
    .await
    .unwrap()
    .0;
    let id = created[0].id.clone();

    let exec_def = ExecutorDefinition {
        id: "bound-exec".into(),
        name: "Bound Exec".into(),
        display_name: "Bound Exec".into(),
        kind: "antigravity".into(),
        command: "antigravity".into(),
        executable: None,
        working_directory: None,
        proxy_id: Some(id.clone()),
        enabled: true,
    };
    state.storage.upsert_executor(exec_def).unwrap();

    let err = delete_proxy(State(state.clone()), Path(id))
        .await
        .unwrap_err();
    assert_eq!(err.0, StatusCode::CONFLICT);
}

/// Test 7: 连接验证确实经由代理发起，失败也会记录最近验证状态
#[tokio::test]
async fn verify_unreachable_proxy_records_failure() {
    let dir = TempDir::new().unwrap();
    let state = test_api_state(dir.path());

    let created = create_proxy(
        State(state.clone()),
        Json(proxy_request("Dead", ProxyKind::Http, "127.0.0.1", 1)),
    )
    .await
    .unwrap()
    .0;
    let id = created[0].id.clone();

    let result = verify_proxy(State(state.clone()), Path(id.clone()))
        .await
        .unwrap()
        .0;
    assert!(!result.success, "unreachable proxy must fail");
    assert!(!result.message.is_empty());

    let stored = state.storage.load_proxy(&id).unwrap().unwrap();
    assert_eq!(stored.last_verified_ok, Some(false));
    assert!(stored.last_verified_at.is_some());
    assert!(stored.last_verified_latency_ms.is_some());
}

/// Test 8: 非法验证目标被拒绝，不会发起请求
#[tokio::test]
async fn verify_rejects_invalid_target() {
    let dir = TempDir::new().unwrap();
    let state = test_api_state(dir.path());

    let mut req = proxy_request("BadTarget", ProxyKind::Http, "127.0.0.1", 9);
    req.test_url = Some("ftp://example.com".into());
    let created = create_proxy(State(state.clone()), Json(req)).await.unwrap().0;
    let id = created[0].id.clone();

    let err = verify_proxy(State(state.clone()), Path(id))
        .await
        .unwrap_err();
    assert_eq!(err.0, StatusCode::BAD_REQUEST);
}

/// Test 9: migration 新增的代理元数据字段可正常读写
#[test]
fn proxy_metadata_round_trips_through_storage() {
    let storage = common::test_storage();
    let def = ProxyDefinition {
        id: "p1".into(),
        name: "P".into(),
        kind: ProxyKind::Http,
        host: "h.local".into(),
        port: 8080,
        username: None,
        password: None,
        enabled: true,
        is_default: true,
        test_url: "https://health.local".into(),
        last_verified_at: None,
        last_verified_ok: None,
        last_verified_latency_ms: None,
    };
    storage.upsert_proxy(def).unwrap();
    storage.record_proxy_verification("p1", true, 42).unwrap();

    let loaded = storage.load_proxy("p1").unwrap().unwrap();
    assert_eq!(loaded.test_url, "https://health.local");
    assert_eq!(loaded.last_verified_ok, Some(true));
    assert_eq!(loaded.last_verified_latency_ms, Some(42));
    assert!(loaded.last_verified_at.is_some());
}

/// Test 10: 真实路由传入的是 executor kind（如 antigravity/opencode），其代理
/// 解析结果必须与直接使用 executor definition id（如 builtin-antigravity）一致。
#[test]
fn resolve_proxy_by_kind_matches_definition_id() {
    let config = Config::new(std::env::temp_dir());
    let executor = |id: &str, name: &str, kind: &str| ExecutorDefinition {
        id: id.into(),
        name: name.into(),
        display_name: name.into(),
        kind: kind.into(),
        command: kind.into(),
        executable: None,
        working_directory: None,
        proxy_id: Some("default".into()),
        enabled: true,
    };
    let defs = vec![
        executor("builtin-antigravity", "Antigravity", "antigravity"),
        executor("builtin-opencode", "OpenCode", "opencode"),
    ];
    let proxies = vec![ProxyDefinition {
        id: "default".into(),
        name: "Default".into(),
        kind: ProxyKind::Http,
        host: "kind-proxy.local".into(),
        port: 8080,
        username: None,
        password: None,
        enabled: true,
        is_default: true,
        test_url: String::new(),
        last_verified_at: None,
        last_verified_ok: None,
        last_verified_latency_ms: None,
    }];
    let registry = ExecutorRegistry::from_config(&config, &defs, &proxies).unwrap();

    for (kind, id) in [
        ("antigravity", "builtin-antigravity"),
        ("opencode", "builtin-opencode"),
    ] {
        let by_kind = registry
            .resolve_proxy_for(kind)
            .unwrap_or_else(|| panic!("proxy must resolve for kind {kind}"));
        let by_id = registry
            .resolve_proxy_for(id)
            .unwrap_or_else(|| panic!("proxy must resolve for definition id {id}"));
        assert_eq!(by_kind.host, "kind-proxy.local");
        assert_eq!(
            by_kind, by_id,
            "kind {kind} and definition id {id} must resolve the same proxy"
        );
        assert_eq!(
            registry.get(kind).map(|e| e.kind().to_string()),
            Some(kind.to_string()),
            "kind {kind} must route to the matching executor"
        );
    }
}
