//! Antigravity CLI executor tests: registration, config, and Proxy injection.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tempfile::TempDir;

use agentbridge::agent::AgentContext;
use agentbridge::config::{Config, ExecutorDefinition, ExecutorMode, ProxyConfig, ProxyKind};
use agentbridge::executor::proxy::apply_proxy_env;
use agentbridge::executor::{
    parse_command, scan_executor, shared_registry, validate_executor_type, AntigravityExecutor,
    ExecutorError, ExecutorRegistry,
};
use agentbridge::models::StartTaskRequest;
use agentbridge::protocol::C2cPlan;
use agentbridge::storage::proxies::ProxyDefinition;
use agentbridge::task::{PlanInput, TaskRuntime};
use agentbridge::workspace::Workspace;

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
        test_url: String::new(),
        last_verified_at: None,
        last_verified_ok: None,
        last_verified_latency_ms: None,
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
fn antigravity_argv_uses_full_multi_character_tokens() {
    let args = agentbridge::executor::antigravity::cli_args("Read PLAN and implement it");

    assert_eq!(
        args,
        [
            "--dangerously-skip-permissions",
            "-p",
            "Read PLAN and implement it"
        ],
        "argv must match the real `agy --dangerously-skip-permissions -p <prompt>` form"
    );
    assert!(
        args.iter().all(|arg| arg.chars().count() > 1),
        "no argument may be a single character: {args:?}"
    );
    assert!(
        !args.iter().any(|arg| arg.starts_with("-cli")),
        "the invalid -cli flag must never be passed: {args:?}"
    );
}

/// Regression: the executor must select Antigravity's non-interactive print
/// mode via `-p <prompt>` rather than launching the IDE/TUI.
#[test]
fn antigravity_argv_requests_non_interactive_print_mode() {
    let prompt = "Read PLAN and implement it";
    let args = agentbridge::executor::antigravity::cli_args(prompt);

    let flag = args
        .iter()
        .position(|arg| *arg == "-p")
        .expect("argv must select non-interactive print mode with -p");
    assert_eq!(
        args.get(flag + 1).copied(),
        Some(prompt),
        "the prompt must immediately follow -p so it is consumed as the flag value"
    );
    assert!(
        !args
            .iter()
            .any(|arg| *arg == "-i" || *arg == "--prompt-interactive"),
        "the interactive TUI flag must never be passed: {args:?}"
    );
    assert!(
        !args
            .iter()
            .any(|arg| arg.contains("--status") || arg.contains("--version")),
        "no status/version preflight may run at launch: {args:?}"
    );
}

#[test]
fn antigravity_command_stays_a_single_token() {
    let executor =
        AntigravityExecutor::new("builtin-antigravity", "Antigravity", "agy", ExecutorMode::Silent)
            .unwrap();
    assert_eq!(executor.command(), "agy");
    assert_eq!(executor.program(), "agy");
    assert!(executor.extra_args().is_empty());
}

#[test]
fn parse_command_never_splits_into_characters() {
    let (program, args) = parse_command("agy --dangerously-skip-permissions").unwrap();
    assert_eq!(program, "agy");
    assert_eq!(args, ["--dangerously-skip-permissions"]);
    assert!(
        !args.iter().any(|arg| arg.chars().count() == 1),
        "arguments must stay whole tokens: {args:?}"
    );

    // Windows path with spaces remains one program token.
    let (program, args) =
        parse_command(r#""C:\Program Files\agy\agy.exe" -p"#).unwrap();
    assert_eq!(program, r"C:\Program Files\agy\agy.exe");
    assert_eq!(args, ["-p"]);
}

#[test]
fn antigravity_executor_keeps_program_and_arguments_separate() {
    let executor = AntigravityExecutor::new(
        "builtin-antigravity",
        "Antigravity",
        r#""C:\Program Files\agy\agy.exe" --verbose"#,
        ExecutorMode::Silent,
    )
    .unwrap();
    assert_eq!(executor.program(), r"C:\Program Files\agy\agy.exe");
    assert_eq!(executor.extra_args(), ["--verbose"]);
    assert!(
        !executor
            .extra_args()
            .iter()
            .any(|arg| arg.chars().count() == 1)
    );
}

#[cfg(windows)]
fn write_probe_marker_fake(dir: &Path, marker: &Path) -> PathBuf {
    let path = dir.join("probe-antigravity.cmd");
    let body = format!(
        "@echo off\r\necho launched>\"{0}\"\r\nexit /b 0\r\n",
        marker.display()
    );
    std::fs::write(&path, body).unwrap();
    path
}

#[cfg(not(windows))]
fn write_probe_marker_fake(dir: &Path, marker: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("probe-antigravity");
    let body = format!("#!/bin/sh\n: > \"{}\"\nexit 0\n", marker.display());
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn antigravity_discovery_never_launches_the_binary() {
    let dir = TempDir::new().unwrap();
    let marker = dir.path().join("probe_marker.txt");
    let fake = write_probe_marker_fake(dir.path(), &marker);

    let definition = antigravity_definition(fake, None);
    let availability = scan_executor(&definition);

    assert!(
        availability.available,
        "executable on disk must be reported available: {availability:?}"
    );
    assert!(
        !marker.exists(),
        "discovery must not spawn the Antigravity launcher (no --version/--status probe)"
    );
}

#[test]
fn builtin_antigravity_defaults_to_agy_cli() {
    let defs = agentbridge::executor::common_executor_definitions();
    let antigravity = defs
        .iter()
        .find(|def| def.kind == "antigravity")
        .expect("builtin antigravity definition");
    assert_eq!(
        antigravity.command, "agy",
        "built-in Antigravity must launch the agent CLI, not the IDE launcher"
    );
}

/// Regression: when both the Electron launcher (`antigravity.cmd`) and the
/// agent CLI (`agy.exe`) are present, resolution must pick the CLI. This is the
/// exact Windows layout that previously launched the IDE.
#[cfg(windows)]
#[test]
fn antigravity_resolution_prefers_agy_cli_over_antigravity_launcher() {
    let dir = TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("antigravity.cmd"),
        "@echo off\r\nexit /b 0\r\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("agy.exe"), b"fake").unwrap();

    let resolved = agentbridge::executor::antigravity::resolve_executable_in(
        "antigravity",
        &[dir.path().to_path_buf()],
    )
    .expect("the agy CLI must resolve when both entries exist");

    assert_eq!(
        resolved
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_ascii_lowercase(),
        "agy.exe",
        "the Electron launcher must never win over the CLI"
    );
}

#[cfg(windows)]
#[test]
fn antigravity_resolution_finds_agy_exe_from_bare_name() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join("agy.exe"), b"fake").unwrap();

    let resolved = agentbridge::executor::antigravity::resolve_executable_in(
        "agy",
        &[dir.path().to_path_buf()],
    )
    .expect("bare agy must resolve to agy.exe");

    assert_eq!(
        resolved
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_ascii_lowercase(),
        "agy.exe"
    );
}

/// A bare IDE launcher name must not be reported as the CLI when no `agy`
/// entry point exists, on any platform.
#[test]
fn antigravity_resolution_refuses_launcher_without_cli() {
    let dir = TempDir::new().unwrap();
    let launcher = if cfg!(windows) {
        dir.path().join("antigravity.cmd")
    } else {
        dir.path().join("antigravity")
    };
    std::fs::write(&launcher, b"launcher").unwrap();

    let resolved = agentbridge::executor::antigravity::resolve_executable_in(
        "antigravity",
        &[dir.path().to_path_buf()],
    );
    assert!(
        resolved.is_none(),
        "an IDE launcher with no agy CLI must not be treated as the CLI: {resolved:?}"
    );
}

/// An explicit path is always honored and never substituted.
#[test]
fn antigravity_resolution_honors_explicit_path() {
    let dir = TempDir::new().unwrap();
    let explicit = if cfg!(windows) {
        dir.path().join("custom-cli.cmd")
    } else {
        dir.path().join("custom-cli")
    };
    std::fs::write(&explicit, b"cli").unwrap();

    let resolved = agentbridge::executor::antigravity::resolve_executable_in(
        &explicit.to_string_lossy(),
        &[],
    );
    assert_eq!(resolved.as_deref(), Some(explicit.as_path()));
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
    assert!(text.contains("--dangerously-skip-permissions"), "args: {text}");
    assert!(text.contains(" -p "), "args: {text}");
    assert!(
        !text.contains("-cli"),
        "legacy IDE flag must not be passed: {text}"
    );
    assert!(
        !text.contains("--status"),
        "the launch path must not run a status preflight: {text}"
    );
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

fn definition(
    id: &str,
    name: &str,
    kind: &str,
    command: impl Into<PathBuf>,
    proxy_id: Option<&str>,
) -> ExecutorDefinition {
    ExecutorDefinition {
        id: id.into(),
        name: name.into(),
        display_name: name.into(),
        kind: kind.into(),
        command: command.into().to_string_lossy().into_owned(),
        executable: None,
        working_directory: None,
        proxy_id: proxy_id.map(str::to_string),
        enabled: true,
    }
}

/// Fake CLI that records the proxy environment variables it actually received
/// into `capture`, so a real routing run can be asserted after the fact.
#[cfg(windows)]
fn write_env_capture_fake(dir: &Path, stem: &str, capture: &Path) -> PathBuf {
    let path = dir.join(format!("fake-{stem}.cmd"));
    let body = format!(
        "@echo off\r\necho %HTTP_PROXY%>\"{0}\"\r\necho %HTTPS_PROXY%>>\"{0}\"\r\nexit /b 0\r\n",
        capture.display()
    );
    std::fs::write(&path, body).unwrap();
    path
}

#[cfg(not(windows))]
fn write_env_capture_fake(dir: &Path, stem: &str, capture: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(format!("fake-{stem}"));
    let body = format!(
        "#!/bin/sh\necho \"$HTTP_PROXY\" > \"{}\"\necho \"$HTTPS_PROXY\" >> \"{}\"\nexit 0\n",
        capture.display(),
        capture.display()
    );
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// Route a real task through [`TaskRuntime`] with the executor kind used by the
/// project profile (not the executor definition id) and return the proxy values
/// the fake CLI received.
async fn routed_proxy_capture(kind: &str, def_id: &str, def_name: &str, dir: &Path) -> String {
    let capture = dir.join(format!("{kind}_proxy_env.txt"));
    let fake = write_env_capture_fake(dir, kind, &capture);
    let config = Config::new(dir.to_path_buf());
    let def = definition(def_id, def_name, kind, fake, Some("default"));
    let registry = shared_registry(
        ExecutorRegistry::from_config(&config, &[def], &[default_proxy()]).unwrap(),
    );
    let workspace = Arc::new(
        Workspace::open(dir, config.security.max_file_size, config.security.deny_sensitive_files)
            .unwrap(),
    );
    let project = format!("{kind}-routing");
    let runtime = TaskRuntime::new(
        project.clone(),
        workspace,
        kind.to_string(),
        registry,
        ExecutorMode::Silent,
        common::test_storage(),
    )
    .unwrap();

    let req = StartTaskRequest {
        project_name: project,
        goal: "proxy routing by executor kind".into(),
        plan: PlanInput {
            actions: vec!["fake cli records proxy env".into()],
            tests: vec!["read captured proxy env".into()],
            success_criteria: "fake cli receives proxy env".into(),
        },
        skills: Vec::new(),
        executor: None,
        continue_task_id: None,
    };
    runtime.start_task(req).await.unwrap();
    runtime.wait().await.unwrap();

    std::fs::read_to_string(&capture).unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
async fn task_routing_injects_proxy_for_antigravity_kind() {
    let dir = TempDir::new().unwrap();
    let captured =
        routed_proxy_capture("antigravity", "builtin-antigravity", "Antigravity", dir.path()).await;
    assert!(
        captured.contains("proxy.example.com:8080"),
        "proxy env must reach the CLI when routed by antigravity kind: {captured:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn task_routing_injects_proxy_for_opencode_kind() {
    let dir = TempDir::new().unwrap();
    let captured =
        routed_proxy_capture("opencode", "builtin-opencode", "OpenCode", dir.path()).await;
    assert!(
        captured.contains("proxy.example.com:8080"),
        "proxy env must reach the CLI when routed by opencode kind: {captured:?}"
    );
}
