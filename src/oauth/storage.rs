//! OAuth 2.1 凭据与令牌持久化仓储层 (Repository Pattern)

use std::collections::HashMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct RegisteredClient {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub client_name: String,
    pub redirect_uris: Vec<String>,
    pub token_endpoint_auth_method: String,
}

impl RegisteredClient {
    pub fn redirect_allowed(&self, uri: &str) -> bool {
        if self.redirect_uris.is_empty() {
            return is_allowed_redirect(uri);
        }
        self.redirect_uris.iter().any(|u| u == uri)
    }
}

#[derive(Clone)]
pub struct PendingAuth {
    pub client_id: String,
    pub redirect_uri: String,
    pub state: Option<String>,
    pub code_challenge: String,
    pub scope: String,
    pub resource: Option<String>,
    pub expires_at: Instant,
}

#[derive(Clone)]
pub struct AuthCode {
    pub client_id: String,
    pub redirect_uri: String,
    pub code_challenge: String,
    pub scope: String,
    pub resource: Option<String>,
    pub expires_at: Instant,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct IssuedToken {
    pub client_id: String,
    pub scope: String,
    pub expires_at: u64, // UNIX timestamp in seconds
}

#[derive(Serialize, Deserialize, Default)]
struct PersistedTokens {
    access: HashMap<String, IssuedToken>,
    refresh: HashMap<String, IssuedToken>,
}

/// 专职凭据与令牌仓储
pub struct TokenStorage {
    pub clients: HashMap<String, RegisteredClient>,
    pub pending: HashMap<String, PendingAuth>,
    pub codes: HashMap<String, AuthCode>,
    pub access: HashMap<String, IssuedToken>,
    pub refresh: HashMap<String, IssuedToken>,
}

impl TokenStorage {
    /// 启动时从磁盘恢复客户端和未过期的令牌
    pub fn load() -> Self {
        let mut clients = HashMap::new();
        let mut access = HashMap::new();
        let mut refresh = HashMap::new();

        if let Some(home) = dirs::home_dir() {
            let base = home.join(".agentbridge");

            // 读取已注册客户端
            let client_path = base.join("oauth_clients.json");
            if let Ok(content) = std::fs::read_to_string(client_path) {
                if let Ok(saved) = serde_json::from_str::<HashMap<String, RegisteredClient>>(&content) {
                    clients = saved;
                }
            }

            // 读取未过期的持久化令牌
            let token_path = base.join("oauth_tokens.json");
            if let Ok(content) = std::fs::read_to_string(token_path) {
                if let Ok(saved) = serde_json::from_str::<PersistedTokens>(&content) {
                    let now = unix_now();
                    access = saved.access.into_iter().filter(|(_, t)| t.expires_at > now).collect();
                    refresh = saved.refresh.into_iter().filter(|(_, t)| t.expires_at > now).collect();
                }
            }
        }

        Self {
            clients,
            pending: HashMap::new(),
            codes: HashMap::new(),
            access,
            refresh,
        }
    }

    /// 清理过期的临时授权请求、临时授权码和过期令牌
    pub fn cleanup(&mut self) {
        let now_instant = Instant::now();
        let now_unix = unix_now();
        self.pending.retain(|_, v| v.expires_at > now_instant);
        self.codes.retain(|_, v| v.expires_at > now_instant);
        self.access.retain(|_, v| v.expires_at > now_unix);
        self.refresh.retain(|_, v| v.expires_at > now_unix);
    }

    /// 安全持久化客户端列表 (受 0600 权限保护)
    pub fn save_clients(&self) {
        save_secure_json("oauth_clients.json", &self.clients);
    }

    /// 安全持久化令牌列表 (受 0600 权限保护)
    pub fn save_tokens(&self) {
        let payload = PersistedTokens {
            access: self.access.clone(),
            refresh: self.refresh.clone(),
        };
        save_secure_json("oauth_tokens.json", &payload);
    }
}

// -----------------------------------------------------------------------------
// 仓储内部落盘工具（严格保护文件安全权限）
// -----------------------------------------------------------------------------

fn save_secure_json<T: Serialize>(filename: &str, data: &T) {
    let Some(home) = dirs::home_dir() else { return };
    let dir = home.join(".agentbridge");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }

    let path = dir.join(filename);
    let Ok(json_text) = serde_json::to_string_pretty(data) else {
        return;
    };

    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let _ = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .and_then(|mut file| file.write_all(json_text.as_bytes()));
    }
    #[cfg(not(unix))]
    {
        let _ = std::fs::write(path, json_text);
    }
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn is_allowed_redirect(uri: &str) -> bool {
    if uri.contains(['<', '>', ' ', '\n', '\r', '\\']) {
        return false;
    }
    if let Some(rest) = uri.strip_prefix("https://") {
        return !rest.is_empty();
    }
    if let Some(rest) = uri.strip_prefix("http://") {
        let host = rest.split(['/', '?', '#']).next().unwrap_or("");
        let host = host.rsplit('@').next().unwrap_or(host);
        if let Some(inner) = host.strip_prefix('[') {
            let ipv6 = inner.split(']').next().unwrap_or("");
            return ipv6 == "::1";
        }
        let hostname = host.split(':').next().unwrap_or(host);
        return matches!(hostname, "127.0.0.1" | "localhost");
    }
    false
}