//! Local Cloudflare Tunnel helper for the tray console.
//!
//! Quick tunnels (`cloudflared tunnel --url`) print a random
//! `*.trycloudflare.com` hostname. Named tunnels (`tunnel run --token`)
//! keep a stable hostname the user already created in Cloudflare.

use std::io::{BufRead, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::{bail, Context, Result};

use crate::executor;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TunnelKind {
    Quick,
    Named,
}

pub struct TunnelHandle {
    pid: u32,
    child: Option<Child>,
    kind: TunnelKind,
    public_base: Arc<Mutex<Option<String>>>,
    named_host: Option<String>,
}

impl TunnelHandle {
    pub fn kind(&self) -> TunnelKind {
        self.kind
    }

    pub fn public_base(&self) -> Option<String> {
        if self.kind == TunnelKind::Named {
            return self.named_host.clone();
        }
        self.public_base.lock().ok().and_then(|g| g.clone())
    }

    pub fn public_mcp_url(&self) -> Option<String> {
        self.public_base().map(|base| {
            let base = base.trim().trim_end_matches('/');
            if base.ends_with("/mcp") {
                base.to_string()
            } else {
                format!("{base}/mcp")
            }
        })
    }

    pub fn stop(&mut self) {
        let _ = executor::kill_process_tree(self.pid);
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for TunnelHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn installed() -> bool {
    executor::find_executable("cloudflared").is_some()
}

pub fn spawn_quick(local_port: u16) -> Result<TunnelHandle> {
    let url = format!("http://127.0.0.1:{local_port}");
    spawn(
        TunnelKind::Quick,
        &["tunnel", "--url", &url, "--no-autoupdate"],
        None,
    )
}

pub fn spawn_named(token: &str, hostname: Option<&str>) -> Result<TunnelHandle> {
    let token = token.trim();
    if token.is_empty() {
        bail!("named tunnel requires tunnel_token");
    }
    spawn(
        TunnelKind::Named,
        &["tunnel", "run", "--token", token, "--no-autoupdate"],
        hostname
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(normalize_https),
    )
}

fn spawn(kind: TunnelKind, args: &[&str], named_host: Option<String>) -> Result<TunnelHandle> {
    let exe = executor::find_executable("cloudflared")
        .context("cloudflared is not installed or not on PATH")?;
    let mut cmd = Command::new(&exe);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    let mut child = cmd
        .spawn()
        .with_context(|| format!("failed to start {}", exe.display()))?;
    let pid = child.id();
    let public_base = Arc::new(Mutex::new(named_host.clone()));
    if kind == TunnelKind::Quick {
        if let Some(out) = child.stdout.take() {
            spawn_reader(out, public_base.clone());
        }
        if let Some(err) = child.stderr.take() {
            spawn_reader(err, public_base.clone());
        }
    }
    Ok(TunnelHandle {
        pid,
        child: Some(child),
        kind,
        public_base,
        named_host,
    })
}

fn spawn_reader(stream: impl Read + Send + 'static, public_base: Arc<Mutex<Option<String>>>) {
    thread::spawn(move || {
        let reader = BufReader::new(stream);
        for line in reader.lines().flatten() {
            if let Some(url) = parse_quick_tunnel_url(&line) {
                if let Ok(mut guard) = public_base.lock() {
                    *guard = Some(url);
                }
            }
        }
    });
}

fn normalize_https(host: &str) -> String {
    let host = host.trim().trim_end_matches('/');
    if host.starts_with("http://") || host.starts_with("https://") {
        host.to_string()
    } else {
        format!("https://{host}")
    }
}

/// Extract a quick-tunnel hostname from a cloudflared log line.
pub fn parse_quick_tunnel_url(line: &str) -> Option<String> {
    let start = line.find("https://")?;
    let rest = &line[start..];
    let end = rest
        .find(|c: char| c.is_whitespace() || matches!(c, '|' | '"' | '\'' | ')' | ']' | ','))
        .unwrap_or(rest.len());
    let url = rest[..end].trim_end_matches('/');
    if url.contains(".trycloudflare.com") {
        Some(url.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_trycloudflare_from_log_line() {
        let line = "2026-09-04 INF |  https://alpha-beta-hash.trycloudflare.com";
        assert_eq!(
            parse_quick_tunnel_url(line).as_deref(),
            Some("https://alpha-beta-hash.trycloudflare.com")
        );
    }

    #[test]
    fn parses_visit_it_at_banner() {
        let line = "Visit it at: https://lucky-name.trycloudflare.com/";
        assert_eq!(
            parse_quick_tunnel_url(line).as_deref(),
            Some("https://lucky-name.trycloudflare.com")
        );
    }

    #[test]
    fn ignores_unrelated_https() {
        assert!(parse_quick_tunnel_url("see https://example.com/docs").is_none());
        assert!(parse_quick_tunnel_url("no url here").is_none());
    }
}
