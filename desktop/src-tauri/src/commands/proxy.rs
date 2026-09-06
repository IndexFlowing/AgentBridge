use agentbridge::config::ProxyConfig;
use agentbridge::executor::test_proxy;

use crate::commands::load;
use crate::dto::{ProxyData, ProxyInput};

fn proxy_data(cfg: &ProxyConfig) -> ProxyData {
    ProxyData {
        enabled: cfg.enabled,
        kind: cfg.kind,
        host: cfg.host.clone(),
        port: cfg.port,
        username_configured: cfg.username.is_some(),
        password_configured: cfg.password.is_some(),
    }
}

#[tauri::command]
pub fn proxy() -> Result<ProxyData, String> {
    let (cfg, _) = load()?;
    Ok(proxy_data(&cfg.proxy))
}

#[tauri::command]
pub fn save_proxy(input: ProxyInput) -> Result<ProxyData, String> {
    let (mut cfg, path) = load()?;
    let mut next = ProxyConfig {
        enabled: input.enabled,
        kind: input.kind,
        host: input.host.trim().into(),
        port: input.port,
        username: input.username.filter(|v| !v.trim().is_empty()),
        password: input.password.filter(|v| !v.trim().is_empty()),
    };
    if next.username.is_none() { next.username = cfg.proxy.username.clone(); }
    if next.password.is_none() { next.password = cfg.proxy.password.clone(); }
    next.validate().map_err(|e| e.to_string())?;
    cfg.proxy = next;
    cfg.save_to_path(&path).map_err(|e| e.to_string())?;
    Ok(proxy_data(&cfg.proxy))
}

#[tauri::command]
pub async fn test_proxy_connection(input: ProxyInput) -> Result<String, String> {
    let proxy = ProxyConfig {
        enabled: true,
        kind: input.kind,
        host: input.host.trim().into(),
        port: input.port,
        username: input.username.filter(|v| !v.trim().is_empty()),
        password: input.password.filter(|v| !v.trim().is_empty()),
    };
    test_proxy(&proxy).await.map_err(|e| e.to_string())
}