use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::Result;

use crate::config::{self, Config};
use crate::executor;
use crate::git;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Ok,
    Fail,
    Skip,
}

#[derive(Debug, Clone)]
pub struct Check {
    pub status: CheckStatus,
    pub name: String,
    pub detail: String,
}

impl Check {
    fn ok(name: &str, detail: impl Into<String>) -> Self {
        Self {
            status: CheckStatus::Ok,
            name: name.into(),
            detail: detail.into(),
        }
    }
    fn fail(name: &str, detail: impl Into<String>) -> Self {
        Self {
            status: CheckStatus::Fail,
            name: name.into(),
            detail: detail.into(),
        }
    }
    fn skip(name: &str, detail: impl Into<String>) -> Self {
        Self {
            status: CheckStatus::Skip,
            name: name.into(),
            detail: detail.into(),
        }
    }
}

fn probe_agentbridge(host: &str, port: u16) -> bool {
    let Ok(mut stream) = TcpStream::connect((host, port)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
    let req = format!("GET /health HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n");
    if stream.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut buf = String::new();
    let _ = stream.read_to_string(&mut buf);
    buf.contains("agentbridge")
}

pub fn run(config: Option<&Config>, config_path: Option<&Path>) -> Result<Vec<Check>> {
    let mut checks = Vec::new();

    match (config, config_path) {
        (Some(cfg), Some(path)) => {
            checks.push(Check::ok(
                "config",
                format!(
                    "{} (workspace={}, port={})",
                    path.display(),
                    cfg.workspace.display(),
                    cfg.port
                ),
            ));
            if cfg.workspace.is_dir() {
                checks.push(Check::ok(
                    "workspace",
                    format!("exists: {}", cfg.workspace.display()),
                ));
            } else {
                checks.push(Check::fail(
                    "workspace",
                    format!("does not exist: {}", cfg.workspace.display()),
                ));
            }

            match TcpListener::bind(cfg.listen_addr()) {
                Ok(_) => checks.push(Check::ok(
                    "port",
                    format!("{} is available", cfg.listen_addr()),
                )),
                Err(err) => {
                    if probe_agentbridge(&cfg.host, cfg.port) {
                        checks.push(Check::ok(
                            "port",
                            format!(
                                "{} in use by a running AgentBridge (http://{}/health)",
                                cfg.listen_addr(),
                                cfg.listen_addr()
                            ),
                        ));
                    } else {
                        checks.push(Check::fail(
                            "port",
                            format!("{} is not available ({err})", cfg.listen_addr()),
                        ));
                    }
                }
            }

            if cfg.is_loopback() {
                checks.push(Check::ok(
                    "bind",
                    format!(
                        "{} (localhost only — use a tunnel for remote MCP)",
                        cfg.host
                    ),
                ));
            } else {
                checks.push(Check::skip(
                    "bind",
                    format!(
                        "{} is not loopback; this exposes the workspace. Prefer 127.0.0.1 plus Cloudflare Tunnel.",
                        cfg.host
                    ),
                ));
            }

            if cfg.auth_token.as_ref().is_some_and(|t| !t.is_empty()) {
                checks.push(Check::ok("auth", "auth_token is set"));
            } else {
                checks.push(Check::skip(
                    "auth",
                    "no auth_token (acceptable for localhost; set one before public tunnels)",
                ));
            }

            checks.push(executor_check(cfg));
        }
        _ => {
            checks.push(Check::fail(
                "config",
                "no config found. Run `agentbridge init <workspace>`.",
            ));
            checks.push(executor_check_command("opencode"));
        }
    }

    match git::git_version() {
        Some(v) => checks.push(Check::ok("git", v)),
        None => checks.push(Check::fail(
            "git",
            "git is not available on PATH (required for git_status / git_diff)",
        )),
    }

    match Command::new("cloudflared").arg("--version").output() {
        Ok(out) if out.status.success() => {
            let version = String::from_utf8_lossy(&out.stdout);
            let line = version.lines().next().unwrap_or("cloudflared").trim();
            checks.push(Check::ok("cloudflared", format!("{line} (optional)")));
        }
        _ => checks.push(Check::skip(
            "cloudflared",
            "not found (optional; used to expose localhost MCP to a remote Brain)",
        )),
    }

    if let Ok(user) = config::user_config_path() {
        if user.is_file() {
            checks.push(Check::ok("user-config", user.display().to_string()));
        }
    }

    Ok(checks)
}

fn executor_check(cfg: &Config) -> Check {
    if let Err(err) = executor::validate_executor_type(&cfg.executor.kind) {
        return Check::fail("opencode", err.to_string());
    }
    executor_check_command(&cfg.executor.command)
}

fn executor_check_command(command: &str) -> Check {
    match executor::find_executable(command) {
        Some(path) => {
            let detail =
                executor::opencode_version(command).unwrap_or_else(|| path.display().to_string());
            Check::ok("opencode", format!("installed: yes ({detail})"))
        }
        None => Check::fail(
            "opencode",
            format!("installed: no (looked for `{command}` on PATH)"),
        ),
    }
}

pub fn print_report(checks: &[Check]) -> bool {
    println!("AgentBridge doctor\n");
    let mut required_ok = true;
    for check in checks {
        let mark = match check.status {
            CheckStatus::Ok => "ok  ",
            CheckStatus::Fail => {
                required_ok = false;
                "FAIL"
            }
            CheckStatus::Skip => "skip",
        };
        println!("  [{mark}] {:<12} {}", check.name, check.detail);
    }
    println!();
    if required_ok {
        println!("Required checks passed.");
    } else {
        println!("Some required checks failed.");
    }
    required_ok
}
