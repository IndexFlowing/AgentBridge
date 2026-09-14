//! Provider Core tests: data model, storage, credentials, registry and API.

use axum::extract::State;
use axum::Json;
use std::sync::Arc;

use agentbridge::api::providers::{list_providers, save_credential, save_model, save_provider};
use agentbridge::api::ApiState;
use agentbridge::config::Config;
use agentbridge::credentials::CredentialCipher;
use agentbridge::models::{CredentialInput, ModelInput, ProviderInput};
use agentbridge::oauth::{OauthServer, OauthSettings};
use agentbridge::provider::{
    reload_provider_registry, shared_provider_registry, ModelDefinition, ProviderDefinition,
    ProviderRegistry,
};
use agentbridge::storage::Storage;

mod common;

fn test_storage() -> Arc<Storage> {
    let dir = common::leak_tempdir();
    Arc::new(
        Storage::open_with_cipher(
            dir.join("agentbridge.db"),
            CredentialCipher::from_passphrase("test-passphrase"),
        )
        .unwrap(),
    )
}

fn test_state(workspace: &std::path::Path) -> ApiState {
    let config = Arc::new(Config::new(workspace.to_path_buf()));
    let storage = test_storage();
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

fn provider_input(id: Option<&str>, name: &str, is_default: bool) -> ProviderInput {
    ProviderInput {
        id: id.map(str::to_string),
        name: name.to_string(),
        kind: "openai".to_string(),
        base_url: Some("https://api.example.com".to_string()),
        enabled: Some(true),
        is_default: Some(is_default),
        proxy_id: None,
        api_key: None,
    }
}

#[test]
fn provider_input_debug_redacts_api_key() {
    let mut input = provider_input(None, "P", false);
    input.api_key = Some("debug-secret".to_string());
    let rendered = format!("{input:?}");
    assert!(!rendered.contains("debug-secret"));
    assert!(rendered.contains("***"));
}

#[test]
fn migration_creates_provider_tables_on_fresh_database() {
    let storage = test_storage();
    assert!(storage.load_providers().unwrap().is_empty());
    assert!(storage.load_models().unwrap().is_empty());

    let provider = ProviderDefinition::new("OpenCode", "opencode");
    storage.upsert_provider(provider.clone()).unwrap();
    let model = ModelDefinition::new(provider.id.clone(), "gpt-4o");
    storage.upsert_model(model.clone()).unwrap();

    assert_eq!(storage.load_providers().unwrap().len(), 1);
    assert_eq!(
        storage
            .load_models_for_provider(&provider.id)
            .unwrap()
            .len(),
        1
    );
    assert!(storage
        .provider_credential_metadata(&provider.id)
        .unwrap()
        .is_none());
}

#[test]
fn provider_crud_enforces_single_default() {
    let storage = test_storage();
    let mut a = ProviderDefinition::new("Provider A", "openai");
    a.is_default = true;
    storage.upsert_provider(a.clone()).unwrap();

    let mut b = ProviderDefinition::new("Provider B", "anthropic");
    b.is_default = true;
    storage.upsert_provider(b.clone()).unwrap();

    let providers = storage.load_providers().unwrap();
    assert_eq!(providers.len(), 2);
    let defaults: Vec<_> = providers.iter().filter(|p| p.is_default).collect();
    assert_eq!(defaults.len(), 1, "at most one default provider");
    assert_eq!(defaults[0].id, b.id);

    let fetched = storage.get_provider(&a.id).unwrap().unwrap();
    assert_eq!(fetched.name, "Provider A");
    assert!(!fetched.is_default);

    storage.delete_provider(&a.id).unwrap();
    assert_eq!(storage.load_providers().unwrap().len(), 1);
    assert!(storage.get_provider(&a.id).unwrap().is_none());
}

#[test]
fn model_crud_associates_with_provider() {
    let storage = test_storage();
    let provider = ProviderDefinition::new("P", "openai");
    storage.upsert_provider(provider.clone()).unwrap();

    let m1 = ModelDefinition::new(provider.id.clone(), "alpha");
    let m2 = ModelDefinition::new(provider.id.clone(), "beta");
    storage.upsert_model(m1.clone()).unwrap();
    storage.upsert_model(m2.clone()).unwrap();
    assert_eq!(
        storage
            .load_models_for_provider(&provider.id)
            .unwrap()
            .len(),
        2
    );

    storage.delete_model(&m1.id).unwrap();
    let remaining = storage.load_models_for_provider(&provider.id).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].name, "beta");

    // Models require an existing provider.
    let orphan = ModelDefinition::new("missing-provider", "x");
    assert!(storage.upsert_model(orphan).is_err());
}

#[tokio::test]
async fn credential_is_encrypted_at_rest_and_never_returned() {
    let dir = tempfile::TempDir::new().unwrap();
    let state = test_state(dir.path());
    let secret = "sk-super-secret-value";

    let mut input = provider_input(None, "Secure", true);
    input.api_key = Some(secret.to_string());
    let saved = save_provider(State(state.clone()), Json(input))
        .await
        .unwrap();
    let id = saved.0[0].id.clone();

    // API response carries metadata, not the secret.
    let listed = list_providers(State(state.clone())).await.unwrap();
    let json = serde_json::to_string(&listed.0).unwrap();
    assert!(!json.contains(secret));
    assert!(listed.0[0].credential_configured);

    // Raw SQLite payload is encrypted.
    let raw = state
        .storage
        .provider_secret_ciphertext(&id)
        .unwrap()
        .unwrap();
    assert!(!raw.contains(secret));
    assert!(raw.starts_with("aes-256-gcm:"));

    // Round-trips through the credential store.
    assert_eq!(
        state.storage.get_provider_secret(&id).unwrap().as_deref(),
        Some(secret)
    );
}

#[tokio::test]
async fn credential_preserved_when_omitted_and_updated_when_explicit() {
    let dir = tempfile::TempDir::new().unwrap();
    let state = test_state(dir.path());

    let mut create = provider_input(None, "P", false);
    create.api_key = Some("first-secret".to_string());
    create.enabled = Some(false);
    let saved = save_provider(State(state.clone()), Json(create))
        .await
        .unwrap();
    let id = saved.0[0].id.clone();

    // Ordinary field update with no secret must preserve it.
    let mut update = provider_input(Some(&id), "P renamed", false);
    update.base_url = Some("https://changed.example.com".to_string());
    let _ = save_provider(State(state.clone()), Json(update))
        .await
        .unwrap();
    assert_eq!(
        state.storage.get_provider_secret(&id).unwrap().as_deref(),
        Some("first-secret")
    );

    // Blank secret also preserves it.
    let mut blank = provider_input(Some(&id), "P renamed", false);
    blank.api_key = Some("   ".to_string());
    let _ = save_provider(State(state.clone()), Json(blank))
        .await
        .unwrap();
    assert_eq!(
        state.storage.get_provider_secret(&id).unwrap().as_deref(),
        Some("first-secret")
    );

    // Explicit new secret replaces it.
    let mut rotate = provider_input(Some(&id), "P renamed", false);
    rotate.api_key = Some("second-secret".to_string());
    let _ = save_provider(State(state.clone()), Json(rotate))
        .await
        .unwrap();
    assert_eq!(
        state.storage.get_provider_secret(&id).unwrap().as_deref(),
        Some("second-secret")
    );

    // Dedicated credential endpoint updates; clearing removes.
    let rotated = save_credential(
        State(state.clone()),
        axum::extract::Path(id.clone()),
        Json(CredentialInput {
            api_key: Some("third-secret".to_string()),
        }),
    )
    .await
    .unwrap();
    assert!(rotated.0.credential_configured);
    assert_eq!(
        state.storage.get_provider_secret(&id).unwrap().as_deref(),
        Some("third-secret")
    );

    state.storage.delete_provider_secret(&id).unwrap();
    assert!(state.storage.get_provider_secret(&id).unwrap().is_none());
    let after = list_providers(State(state.clone())).await.unwrap();
    assert!(!after.0[0].credential_configured);
}

#[tokio::test]
async fn api_saves_models_and_reloads_registry() {
    let dir = tempfile::TempDir::new().unwrap();
    let state = test_state(dir.path());

    let saved = save_provider(
        State(state.clone()),
        Json(provider_input(None, "Primary", true)),
    )
    .await
    .unwrap();
    let id = saved.0[0].id.clone();

    let _ = save_model(
        State(state.clone()),
        axum::extract::Path(id.clone()),
        Json(ModelInput {
            id: None,
            name: "gpt-4o".to_string(),
            enabled: Some(true),
        }),
    )
    .await
    .unwrap();

    let registry = state.providers.read().unwrap().clone();
    let provider = registry.default_provider().expect("default provider");
    assert_eq!(provider.id, id);
    let model = registry
        .resolve_model(provider, Some("gpt-4o"))
        .expect("model resolves by name");
    assert_eq!(model.name, "gpt-4o");
}

#[test]
fn registry_resolves_default_specific_and_model_then_reloads() {
    let storage = test_storage();
    let mut a = ProviderDefinition::new("Alpha", "openai");
    a.is_default = false;
    storage.upsert_provider(a.clone()).unwrap();
    let mut b = ProviderDefinition::new("Beta", "anthropic");
    b.is_default = true;
    storage.upsert_provider(b.clone()).unwrap();

    let ma = ModelDefinition::new(a.id.clone(), "alpha-model");
    let mb = ModelDefinition::new(b.id.clone(), "beta-model");
    storage.upsert_model(ma.clone()).unwrap();
    storage.upsert_model(mb.clone()).unwrap();

    let shared = shared_provider_registry(ProviderRegistry::from_storage(&storage).unwrap());
    {
        let registry = shared.read().unwrap().clone();
        assert_eq!(registry.default_provider().unwrap().id, b.id);
        assert_eq!(registry.resolve_provider(None).unwrap().id, b.id);
        assert_eq!(registry.resolve_provider(Some("default")).unwrap().id, b.id);
        assert_eq!(registry.resolve_provider(Some("Alpha")).unwrap().id, a.id);
        assert_eq!(registry.resolve_provider(Some(&a.id)).unwrap().id, a.id);

        let alpha = registry.resolve_provider(Some("Alpha")).unwrap();
        assert_eq!(
            registry.resolve_model(alpha, None).unwrap().id,
            ma.id,
            "falls back to first enabled model"
        );
        assert_eq!(
            registry
                .resolve_model(alpha, Some("alpha-model"))
                .unwrap()
                .id,
            ma.id
        );
    }

    // Promote Alpha to default and hot-reload the shared registry.
    let mut updated_a = a.clone();
    updated_a.is_default = true;
    storage.upsert_provider(updated_a).unwrap();
    reload_provider_registry(&shared, &storage).unwrap();

    let registry = shared.read().unwrap().clone();
    assert_eq!(registry.default_provider().unwrap().id, a.id);
    assert_eq!(registry.len(), 2);
}
