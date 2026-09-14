// src/config/proxy.rs
use anyhow::bail;
use serde::{Deserialize, Serialize};

use crate::config::patch::keep_secret;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProxyConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub kind: ProxyKind,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: u16,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProxyKind {
    Http,
    Https,
    Socks5,
}

impl Default for ProxyKind {
    fn default() -> Self {
        Self::Http
    }
}

impl ProxyKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
            Self::Socks5 => "socks5",
        }
    }

    pub fn from_str_opt(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "https" => Self::Https,
            "socks5" | "socks" | "socks5h" => Self::Socks5,
            _ => Self::Http,
        }
    }
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            kind: ProxyKind::Http,
            host: String::new(),
            port: 0,
            username: None,
            password: None,
        }
    }
}

impl ProxyConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        if !self.enabled {
            return Ok(());
        }
        if self.host.trim().is_empty() {
            bail!("proxy host is required");
        }
        if self.port == 0 {
            bail!("proxy port is required");
        }
        Ok(())
    }

    pub fn url(&self) -> anyhow::Result<String> {
        self.validate()?;
        let scheme = match self.kind {
            ProxyKind::Http => "http",
            ProxyKind::Https => "https",
            ProxyKind::Socks5 => "socks5h",
        };
        let auth = match (&self.username, &self.password) {
            (Some(user), Some(password)) if !user.is_empty() => {
                format!("{}:{}@", encode_url_part(user), encode_url_part(password))
            }
            (Some(user), _) if !user.is_empty() => format!("{}@", encode_url_part(user)),
            _ => String::new(),
        };
        Ok(format!(
            "{scheme}://{auth}{}:{}",
            self.host.trim(),
            self.port
        ))
    }
}

fn encode_url_part(value: &str) -> String {
    value.bytes().map(|b| format!("%{b:02X}")).collect()
}

#[derive(Debug, Clone, Default)]
pub struct ProxyPatch {
    pub enabled: Option<bool>,
    pub kind: Option<ProxyKind>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProxyView {
    pub enabled: bool,
    pub kind: ProxyKind,
    pub host: String,
    pub port: u16,
    pub username_configured: bool,
    pub password_configured: bool,
}

pub fn apply_proxy_patch(current: &ProxyConfig, patch: ProxyPatch) -> anyhow::Result<ProxyConfig> {
    let mut next = current.clone();
    if let Some(enabled) = patch.enabled {
        next.enabled = enabled;
    }
    if let Some(kind) = patch.kind {
        next.kind = kind;
    }
    if let Some(host) = patch.host {
        next.host = host.trim().to_string();
    }
    if let Some(port) = patch.port {
        next.port = port;
    }
    next.username = keep_secret(patch.username, current.username.clone());
    next.password = keep_secret(patch.password, current.password.clone());
    next.validate()?;
    Ok(next)
}

pub fn proxy_view(proxy: &ProxyConfig) -> ProxyView {
    ProxyView {
        enabled: proxy.enabled,
        kind: proxy.kind,
        host: proxy.host.clone(),
        port: proxy.port,
        username_configured: proxy.username.is_some(),
        password_configured: proxy.password.is_some(),
    }
}
