//! Antigravity CLI executor tests: registration, config, and Proxy injection.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use agentbridge::agent::AgentContext;
use agentbridge::config::{Config, ExecutorDefinition, ProxyConfig, ProxyKind};
use agentbridge::executor::proxy::apply_proxy_env;
use agentbridge::executor::{validate_executor_type, ExecutorError, ExecutorRegistry};
use agentbridge::protocol::C2cPlan;
use agentbridge::storage::proxies::ProxyDefinition;

mod common;

fn antigravity_definition(command: impl Into<PathBuf>, proxy_id: Option<&str>) -> ExecutorDefinition {
    ExecutorDefinition {
        id: "builtin-antigravity".into(),
        name: "Antigravity".into(),
        display_name: "Antigravity".into(),
        kind: "antigravity".into(),
        command: command.into().to_string_lossy().into_owned(),
        executable: None,
        working_directory: None,
        proxy_id: proxy_id.map(str::to_string),
        enabled: true,
    }
}

fn default_proxy() -> ProxyDefinition {
    ProxyDefinition {
        id: "default".into(),
        name: "Default Proxy".into(),
        kind: ProxyKind::Http,
        host: "proxy.example.com".into(),
        port: 8080,
        username: None,
        password: None,
        enabled: true,
        is_default: true,
    }
}

fn sample_plan() -> C2cPlan {
    C2cPlan::new(
        "c2c_antigravity_test".into(),
        1,
        "Antigravity smoke test".into(),
        vec!["Inspect the workspace.".into()],
        vec!["cargo test".into()],
        "The executor is launched.".into(),
    )
    .unwrap()
}

#[cfg(windows)]
fn write_fake(dir: &Path) -> PathBuf {
    let path = dir.join("fake-antigravity.cmd");
    std::fs::write(
        &path,
        "@echo off\r\necho AG_ARGS %*\r\necho AG_PROXY %HTTP_PROXY%\r\nexit /b 0\r\n",
    )
    .unwrap();
    path
}

#[cfg(not(windows))]
fn write_fake(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("fake-antigravity");
    std::fs::write(
        &path,
        "#!/bin/sh\necho \"AG_ARGS $*\"\necho \"AG_PROXY $HTTP_PROXY\"\nexit 0\n",
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn antigravity_is_allowlisted_and_implemented() {
    assert!(validate_executor_type("antigravity").is_ok());
    assert!(validate_executor_type(" antigravity ").is_ok());
    assert!(validate_executor_type("opencode").is_ok());
    assert!(matches!(
        validate_executor_type("codex"),
        Err(ExecutorError::TypeNotImplemented(_))
    ));
    assert!(matches!(
        validate_executor_type("not-a-real-executor"),
        Err(ExecutorError::TypeNotAllowed(_))
    ));
}

#[test]
fn registry_registers_antigravity_from_definition() {
    let dir = TempDir::new().unwrap();
    let config = Config::new(dir.path().to_path_buf());
    let registry =
        ExecutorRegistry::from_config(&config, &[antigravity_definition("antigravity", None)], &[])
            .unwrap();

    let executor = registry
        .get("builtin-antigravity")
        .expect("antigravity registered by id");
    assert_eq!(executor.kind(), "antigravity");
    assert!(registry.get("Antigravity").is_some(), "name alias");
    assert!(registry.get("antigravity").is_some(), "canonical kind key");
}

#[test]
fn config_antigravity_kind_registers_default_executor() {
    let dir = TempDir::new().unwrap();
    let mut config = Config::new(dir.path().to_path_buf());
    config.executor.kind = "antigravity".into();
    config.executor.command = "antigravity".into();

    let registry = ExecutorRegistry::from_config(&config, &[], &[]).unwrap();
    let executor = registry
        .get("antigravity")
        .expect("config.toml antigravity fallback must register");
    assert_eq!(executor.kind(), "antigravity");
}

#[test]
fn antigravity_definition_roundtrips_through_storage() {
    let storage = common::test_storage();
    storage
        .upsert_executor(antigravity_definition("antigravity", Some("default")))
        .unwrap();

    let loaded = storage.load_executors().unwrap();
    let found = loaded
        .into_iter()
        .find(|def| def.id == "builtin-antigravity")
        .expect("persisted antigravity definition");

    assert_eq!(found.kind, "antigravity");
    assert_eq!(found.command, "antigravity");
    assert_eq!(found.proxy_id.as_deref(), Some("default"));
    assert!(found.enabled);
}

#[test]
fn apply_proxy_env_injects_all_proxy_variables() {
    let proxy = ProxyConfig {
        enabled: true,
        kind: ProxyKind::Socks5,
        host: "p.local".into(),
        port: 1080,
        username: None,
        password: None,
    };
    let mut cmd = std::process::Command::new("antigravity");
    apply_proxy_env(&mut cmd, Some(&proxy)).unwrap();

    let vars = env_map(&cmd);
    assert_eq!(
        vars.get("HTTP_PROXY").map(String::as_str),
        Some("socks5h://p.local:1080")
    );
    assert_eq!(
        vars.get("HTTPS_PROXY").map(String::as_str),
        Some("socks5h://p.local:1080")
    );
    assert_eq!(
        vars.get("ALL_PROXY").map(String::as_str),
        Some("socks5h://p.local:1080")
    );
    assert_eq!(
        vars.get("NO_PROXY").map(String::as_str),
        Some("localhost,127.0.0.1,::1")
    );
}

#[test]
fn apply_proxy_env_none_and_disabled_are_noops() {
    let mut unconfigured = std::process::Command::new("antigravity");
    apply_proxy_env(&mut unconfigured, None).unwrap();
    assert!(env_map(&unconfigured).is_empty());

    let disabled = ProxyConfig {
        enabled: false,
        ..ProxyConfig::default()
    };
    let mut cmd = std::process::Command::new("antigravity");
    apply_proxy_env(&mut cmd, Some(&disabled)).unwrap();
    assert!(env_map(&cmd).is_empty());
}

fn env_map(cmd: &std::process::Command) -> HashMap<String, String> {
    cmd.get_envs()
        .map(|(key, value)| {
            (
                key.to_string_lossy().into_owned(),
                value.unwrap_or_default().to_string_lossy().into_owned(),
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn antigravity_start_passes_cli_args_and_proxy_env() {
    let dir = TempDir::new().unwrap();
    let fake = write_fake(dir.path());
    let config = Config::new(dir.path().to_path_buf());

    let registry = ExecutorRegistry::from_config(
        &config,
        &[antigravity_definition(fake, Some("default"))],
        &[default_proxy()],
    )
    .unwrap();

    let executor = registry
        .get("builtin-antigravity")
        .expect("antigravity registered");
    let proxy = registry
        .resolve_proxy_for("builtin-antigravity")
        .expect("default proxy must resolve for antigravity");

    let spawned = executor
        .start_task(
            &sample_plan(),
            &AgentContext::default(),
            dir.path(),
            Some(&proxy),
        )
        .unwrap();
    let output = spawned.child.wait_with_output().await.unwrap();

    assert!(output.status.success(), "fake antigravity must exit 0");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("-cli"), "args: {text}");
    assert!(text.contains("-print"), "args: {text}");
    assert!(
        text.contains("proxy.example.com:8080"),
        "proxy env not forwarded: {text}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn antigravity_start_without_proxy_resolves_none() {
    let dir = TempDir::new().unwrap();
    let fake = write_fake(dir.path());
    let config = Config::new(dir.path().to_path_buf());

    let registry = ExecutorRegistry::from_config(
        &config,
        &[antigravity_definition(fake, None)],
        &[],
    )
    .unwrap();

    assert!(registry.resolve_proxy_for("builtin-antigravity").is_none());

    let executor = registry
        .get("builtin-antigravity")
        .expect("antigravity registered");
    let spawned = executor
        .start_task(&sample_plan(), &AgentContext::default(), dir.path(), None)
        .unwrap();
    let output = spawned.child.wait_with_output().await.unwrap();
    assert!(output.status.success(), "fake antigravity must exit 0");
}
