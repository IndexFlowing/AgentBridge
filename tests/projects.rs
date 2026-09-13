//! Multi-project isolation: tools targeting one project cannot read another.

use std::fs;
use std::sync::Arc;

use agentbridge::config::{self, Config, ExecutorDefinition, ExecutorRegistryFile};
use agentbridge::mcp::eval_tool;
use agentbridge::projects::{
    load_workspaces_file, project_id, save_workspaces_file, upsert_project, ProjectEntry, ProjectHub,
    ProjectUpsert,
};
use serde_json::json;
use tempfile::TempDir;

fn two_projects() -> (TempDir, TempDir, TempDir, ProjectHub) {
    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    fs::create_dir_all(a.path().join("src")).unwrap();
    fs::write(a.path().join("src/main.rs"), "fn alpha() {}\n").unwrap();
    fs::write(a.path().join("secret-a.txt"), "AAA\n").unwrap();
    fs::create_dir_all(b.path().join("src")).unwrap();
    fs::write(b.path().join("src/main.rs"), "fn beta() {}\n").unwrap();
    fs::write(b.path().join("secret-b.txt"), "BBB\n").unwrap();

    let cfg_dir = TempDir::new().unwrap();
    let json = cfg_dir.path().join("agentbridge.config.json");
    fs::write(
        &json,
        serde_json::json!({
            "projects": [
                {"name": "alpha", "path": a.path(), "description": "Project A"},
                {"name": "beta", "path": b.path(), "description": "Project B"}
            ],
            "default_project": "alpha"
        })
        .to_string(),
    )
    .unwrap();
    let (entries, default) = load_workspaces_file(&json).unwrap();
    let cfg = Arc::new(Config::new(a.path().to_path_buf()));
    let hub = ProjectHub::open_with_path(entries, default, cfg, json).unwrap();
    (a, b, cfg_dir, hub)
}

#[test]
fn list_contains_both_and_marks_default_active() {
    let (_a, _b, _cfg, hub) = two_projects();
    let list = hub.list(hub.default_name());
    assert_eq!(list.len(), 2);
    assert_eq!(hub.default_name(), "alpha");
    assert!(list.iter().any(|p| p.name == "alpha" && p.active));
    assert!(list.iter().any(|p| p.name == "beta" && !p.active));
}

#[test]
fn read_file_stays_inside_selected_project() {
    let (_a, _b, _cfg, hub) = two_projects();
    let alpha = hub.get("alpha").unwrap();
    let beta = hub.get("beta").unwrap();

    let a_main = eval_tool(&alpha.workspace, "read_file", json!({"path": "src/main.rs"})).unwrap();
    assert!(a_main["content"].as_str().unwrap().contains("alpha"));

    let b_main = eval_tool(&beta.workspace, "read_file", json!({"path": "src/main.rs"})).unwrap();
    assert!(b_main["content"].as_str().unwrap().contains("beta"));

    let err = eval_tool(
        &alpha.workspace,
        "read_file",
        json!({"path": "../secret-b.txt"}),
    )
    .unwrap_err();
    assert!(err.contains("outside the workspace"), "{err}");

    assert!(alpha.workspace.read_file("secret-b.txt").is_err());
    assert_eq!(beta.workspace.read_file("secret-b.txt").unwrap(), "BBB\n");
}

#[test]
fn unknown_project_is_rejected() {
    let (_a, _b, _cfg, hub) = two_projects();
    assert!(hub.get("does-not-exist").is_none());
    assert!(hub.get("alpha").is_some());
    assert!(hub.get("ALPHA").is_some());
}

#[test]
fn traversal_cannot_reach_sibling_project() {
    let (a, b, _cfg, hub) = two_projects();
    let alpha = hub.get("alpha").unwrap();
    let rel = pathdiff_naive(a.path(), b.path());
    if let Some(rel) = rel {
        let err = alpha
            .workspace
            .read_file(&format!("{rel}/secret-b.txt"))
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("outside the workspace") || msg.contains("not found"),
            "{msg}"
        );
    }
}

fn pathdiff_naive(from: &std::path::Path, to: &std::path::Path) -> Option<String> {
    let from = from.canonicalize().ok()?;
    let to = to.canonicalize().ok()?;
    let shared = from
        .components()
        .zip(to.components())
        .take_while(|(a, b)| a == b)
        .count();
    let ups = from.components().count().saturating_sub(shared);
    let downs: Vec<_> = to.components().skip(shared).collect();
    let mut out = std::path::PathBuf::new();
    for _ in 0..ups {
        out.push("..");
    }
    for d in downs {
        out.push(d);
    }
    Some(out.to_string_lossy().replace('\\', "/"))
}

#[test]
fn project_crud_persists_stable_id_without_touching_directory() {
    let workspace = TempDir::new().unwrap();
    let config_dir = TempDir::new().unwrap();
    let config = config_dir.path().join("agentbridge.config.json");
    let original = workspace.path().join("keep.txt");
    fs::write(&original, "keep").unwrap();
    let mut entry = ProjectEntry {
        id: String::new(),
        name: "alpha".into(),
        path: workspace.path().to_path_buf(),
        description: String::new(),
        readonly: false,
        executor: "opencode".into(),
    };
    save_workspaces_file(&config, &[entry.clone()], Some("alpha".into())).unwrap();
    let (loaded, _) = load_workspaces_file(&config).unwrap();
    let id = loaded[0].id.clone();
    assert_eq!(id, project_id(&loaded[0]));

    entry.id = id.clone();
    entry.name = "renamed".into();
    entry.executor = "opencode".into();
    save_workspaces_file(&config, &[entry], Some("renamed".into())).unwrap();
    let (edited, default) = load_workspaces_file(&config).unwrap();
    assert_eq!(edited[0].id, id);
    assert_eq!(edited[0].name, "renamed");
    assert_eq!(default.as_deref(), Some("renamed"));

    fs::remove_file(&config).unwrap();
    assert_eq!(fs::read_to_string(original).unwrap(), "keep");
}

#[test]
fn upsert_project_is_the_single_write_path() {
    let workspace = TempDir::new().unwrap();
    let config_dir = TempDir::new().unwrap();
    let file = config_dir.path().join("agentbridge.config.json");
    let (entries, default) = upsert_project(
        &file,
        ProjectUpsert {
            name: "alpha".into(),
            path: workspace.path().to_path_buf(),
            description: Some("A".into()),
            executor: Some("opencode".into()),
            make_default: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(default.as_deref(), Some("alpha"));
    let id = entries[0].id.clone();
    assert!(!id.is_empty());

    let (edited, default) = upsert_project(
        &file,
        ProjectUpsert {
            id: Some(id.clone()),
            name: "renamed".into(),
            path: workspace.path().to_path_buf(),
            executor: Some("opencode".into()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(edited[0].id, id);
    assert_eq!(edited[0].name, "renamed");
    assert_eq!(edited[0].description, "A");
    assert_eq!(default.as_deref(), Some("renamed"));
}

#[test]
fn hub_loads_executors_from_config_path_not_dot() {
    let workspace = TempDir::new().unwrap();
    let config_dir = TempDir::new().unwrap();
    let other_dir = TempDir::new().unwrap();
    let config_path = config_dir.path().join("config.toml");
    Config::new(workspace.path().to_path_buf())
        .save_to_path(&config_path)
        .unwrap();

    let mut from_config = ExecutorDefinition::new(
        "From Config Dir".into(),
        "opencode".into(),
        "opencode".into(),
    );
    from_config.id = "from-config-dir".into();
    config::save_executor_registry(
        &config_path,
        &ExecutorRegistryFile {
            executors: vec![from_config],
        },
    )
    .unwrap();

    let other_config = other_dir.path().join("config.toml");
    let mut from_other = ExecutorDefinition::new("From Other".into(), "opencode".into(), "opencode".into());
    from_other.id = "from-other-dir".into();
    config::save_executor_registry(
        &other_config,
        &ExecutorRegistryFile {
            executors: vec![from_other],
        },
    )
    .unwrap();

    let cfg = Arc::new(Config::new(workspace.path().to_path_buf()));
    let hub = ProjectHub::open_with_path(
        vec![ProjectEntry {
            id: String::new(),
            name: "alpha".into(),
            path: workspace.path().to_path_buf(),
            description: String::new(),
            readonly: false,
            executor: "opencode".into(),
        }],
        Some("alpha".into()),
        cfg,
        config_path.clone(),
    )
    .unwrap();
    assert_eq!(hub.config_path(), config_path.as_path());
    assert!(hub.has_executor("from-config-dir"));
    assert!(!hub.has_executor("from-other-dir"));
}

#[test]
fn discover_with_config_path_uses_sidecar() {
    let workspace = TempDir::new().unwrap();
    let config_dir = TempDir::new().unwrap();
    let config_path = config_dir.path().join("config.toml");
    Config::new(workspace.path().to_path_buf())
        .save_to_path(&config_path)
        .unwrap();

    let sidecar = config_dir.path().join("agentbridge.config.json");
    save_workspaces_file(
        &sidecar,
        &[ProjectEntry {
            id: String::new(),
            name: "from-config".into(),
            path: workspace.path().to_path_buf(),
            description: String::new(),
            readonly: false,
            executor: "opencode".into(),
        }],
        Some("from-config".into()),
    )
    .unwrap();

    let (entries, default) = agentbridge::projects::discover(
        None,
        None,
        None,
        workspace.path(),
        Some(config_path.as_path()),
    )
    .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "from-config");
    assert_eq!(default.as_deref(), Some("from-config"));
}

#[test]
fn remove_project_refuses_last_entry() {
    let workspace = TempDir::new().unwrap();
    let config_dir = TempDir::new().unwrap();
    let file = config_dir.path().join("agentbridge.config.json");
    upsert_project(
        &file,
        ProjectUpsert {
            name: "only".into(),
            path: workspace.path().to_path_buf(),
            ..Default::default()
        },
    )
    .unwrap();
    let err = agentbridge::projects::remove_project(&file, "only").unwrap_err();
    assert!(err.to_string().contains("at least one project must remain"));
}

#[test]
fn dashboard_snapshot_lists_projects_and_tasks() {
    let (_a, _b, _cfg, hub) = two_projects();
    let cfg = Config::new(hub.get("alpha").unwrap().workspace.root().to_path_buf());
    let gateway = agentbridge::dashboard::gateway_status_with_online(&cfg, false, false, Vec::new());
    let snapshot = agentbridge::dashboard::snapshot(&cfg, &hub, gateway).unwrap();
    assert_eq!(snapshot.project_count, 2);
    assert_eq!(snapshot.projects.len(), 2);
    assert_eq!(snapshot.default_project, "alpha");
    assert_eq!(snapshot.executor.kind, "opencode");
    assert!(snapshot.executor.implemented);
    assert_eq!(snapshot.tasks.len(), 2);
}
