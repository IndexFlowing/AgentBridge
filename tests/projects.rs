//! Multi-project isolation and project persistence against SQLite storage.

use std::fs;
use std::sync::Arc;

use agentbridge::config::Config;
use agentbridge::projects::ProjectHub;
use agentbridge::storage::Storage;
use tempfile::TempDir;

mod common;

fn two_projects() -> (TempDir, TempDir, Arc<Storage>, ProjectHub) {
    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    fs::create_dir_all(a.path().join("src")).unwrap();
    fs::write(a.path().join("src/main.rs"), "fn alpha() {}\n").unwrap();
    fs::write(a.path().join("secret-a.txt"), "AAA\n").unwrap();
    fs::create_dir_all(b.path().join("src")).unwrap();
    fs::write(b.path().join("src/main.rs"), "fn beta() {}\n").unwrap();
    fs::write(b.path().join("secret-b.txt"), "BBB\n").unwrap();

    let storage = common::test_storage();
    let cfg = Arc::new(Config::new(a.path().to_path_buf()));
    let hub = common::hub_with(
        cfg,
        storage.clone(),
        vec![
            common::project_entry("alpha", a.path().to_path_buf()),
            common::project_entry("beta", b.path().to_path_buf()),
        ],
    );
    (a, b, storage, hub)
}

#[test]
fn list_contains_both_and_marks_default_active() {
    let (_a, _b, _storage, hub) = two_projects();
    let list = hub.list(hub.default_name());
    assert_eq!(list.len(), 2);
    assert_eq!(hub.default_name(), "alpha");
    assert!(list.iter().any(|p| p.name == "alpha" && p.active));
    assert!(list.iter().any(|p| p.name == "beta" && !p.active));
}

#[test]
fn read_file_stays_inside_selected_project() {
    let (_a, _b, _storage, hub) = two_projects();
    let alpha = hub.get("alpha").unwrap();
    let beta = hub.get("beta").unwrap();

    let a_main = alpha.workspace.read_file("src/main.rs").unwrap();
    assert!(a_main.contains("alpha"));

    let b_main = beta.workspace.read_file("src/main.rs").unwrap();
    assert!(b_main.contains("beta"));

    let err = alpha.workspace.read_file("../secret-b.txt").unwrap_err();
    assert!(err.to_string().contains("outside the workspace"), "{err}");

    assert!(alpha.workspace.read_file("secret-b.txt").is_err());
    assert_eq!(beta.workspace.read_file("secret-b.txt").unwrap(), "BBB\n");
}

#[test]
fn unknown_project_is_rejected() {
    let (_a, _b, _storage, hub) = two_projects();
    assert!(hub.get("does-not-exist").is_none());
    assert!(hub.get("alpha").is_some());
    assert!(hub.get("ALPHA").is_some());
}

#[test]
fn traversal_cannot_reach_sibling_project() {
    let (a, b, _storage, hub) = two_projects();
    let alpha = hub.get("alpha").unwrap();
    if let Some(rel) = pathdiff_naive(a.path(), b.path()) {
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
fn project_upsert_persists_stable_id_and_delete_keeps_directory() {
    let workspace = TempDir::new().unwrap();
    let original = workspace.path().join("keep.txt");
    fs::write(&original, "keep").unwrap();

    let storage = common::test_storage();
    storage
        .upsert_project(common::project_entry(
            "alpha",
            workspace.path().to_path_buf(),
        ))
        .unwrap();

    let loaded = storage.load_projects().unwrap();
    assert_eq!(loaded.len(), 1);
    let id = loaded[0].id.clone();
    assert!(!id.is_empty());

    let mut edited = loaded[0].clone();
    edited.name = "renamed".into();
    storage.upsert_project(edited).unwrap();
    let loaded = storage.load_projects().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, id);
    assert_eq!(loaded[0].name, "renamed");

    storage.delete_project(&id).unwrap();
    assert!(storage.load_projects().unwrap().is_empty());
    assert_eq!(fs::read_to_string(original).unwrap(), "keep");
}

#[test]
fn dashboard_snapshot_lists_projects_and_tasks() {
    let (a, _b, storage, hub) = two_projects();
    let cfg = Config::new(a.path().to_path_buf());
    let gateway = agentbridge::dashboard::gateway_status(&cfg, false, Vec::new());
    let snapshot = agentbridge::dashboard::snapshot(&cfg, &hub, gateway, &storage).unwrap();
    assert_eq!(snapshot.project_count, 2);
    assert_eq!(snapshot.projects.len(), 2);
    assert_eq!(snapshot.default_project, "alpha");
    assert_eq!(snapshot.executor.kind, "opencode");
    assert!(snapshot.executor.implemented);
    assert_eq!(snapshot.tasks.len(), 2);
}
