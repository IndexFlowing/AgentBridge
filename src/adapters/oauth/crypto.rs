//! OAuth 2.1 加密、PKCE 验证与 URL 协议纯函数工具

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::oauth::SCOPES_SUPPORTED;

/// 计算 PKCE S256 code_challenge: BASE64URL-ENCODE(SHA256(ASCII(code_verifier)))
pub fn pkce_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

/// 恒定时间字符串比对（防时序攻击）
pub fn ct_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// 生成带前缀的安全随机令牌（如 "abt_...", "abc_..."）
pub fn random_token(prefix: &str) -> String {
    format!(
        "{prefix}{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

/// 规范化与过滤客户端请求的 scope
pub fn normalize_scope(scope: Option<&str>) -> String {
    let requested = scope.unwrap_or("mcp:read mcp:write");
    let mut out = Vec::new();
    for part in requested.split_whitespace() {
        if SCOPES_SUPPORTED.contains(&part) && !out.contains(&part) {
            out.push(part);
        }
    }
    if out.is_empty() {
        "mcp:read mcp:write".into()
    } else {
        out.join(" ")
    }
}

pub fn redirect_success(uri: &str, code: &str, state: Option<&str>) -> String {
    append_query(uri, &[("code", Some(code)), ("state", state)])
}

pub fn redirect_error(uri: &str, error: &str, desc: &str, state: Option<&str>) -> String {
    append_query(
        uri,
        &[
            ("error", Some(error)),
            ("error_description", Some(desc)),
            ("state", state),
        ],
    )
}

fn append_query(uri: &str, pairs: &[(&str, Option<&str>)]) -> String {
    let mut out = uri.to_string();
    let mut first = !uri.contains('?');
    for (k, v) in pairs {
        let Some(v) = v else { continue };
        out.push(if first { '?' } else { '&' });
        first = false;
        out.push_str(k);
        out.push('=');
        out.push_str(&form_urlencoded(v));
    }
    out
}

fn form_urlencoded(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
