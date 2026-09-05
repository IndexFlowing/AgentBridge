//! Multi-project isolation: tools targeting one project cannot read another.

use std::fs;
use std::sync::Arc;

use agentbridge::config::Config;
use agentbridge::mcp::eval_tool;
use agentbridge::projects::{load_workspaces_file, project_id, save_workspaces_file, ProjectEntry, ProjectHub};
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
    let hub = ProjectHub::open(entries, default, cfg).unwrap();
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
