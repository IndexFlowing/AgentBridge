use crate::config::{Config, ExecutorMode};
use crate::oauth::OauthServer;
use crate::projects::ProjectHub;

pub fn print_startup_banner(
    config: &Config,
    hub: &ProjectHub,
    oauth: &OauthServer,
    loopback: bool,
    allow_any_host: bool,
    require_auth: bool,
    no_auth_flag: bool,
) {
    let mode_str = match config.executor.mode {
        ExecutorMode::Stream => "stream (live terminal output)",
        ExecutorMode::Silent => "silent (quiet background)",
    };

    let names = hub.names();
    let workspace_line = format!("[{}] ({} mounted)", names.join(", "), hub.len());

    let auth_str = if !require_auth {
        if no_auth_flag {
            "disabled (--no-auth / --dev)"
        } else {
            "disabled"
        }
    } else if oauth.has_static_token() {
        "OAuth 2.1 Enabled (/oauth/authorize) + static Bearer token"
    } else {
        "OAuth 2.1 Enabled (/oauth/authorize)"
    };

    println!();
    println!("╭──────────────────────────────────────────────────────────────────────────╮");
    println!(
        "│   AgentBridge v{:<7} — Autonomous Multi-Agent MCP Bridge                │",
        env!("CARGO_PKG_VERSION")
    );
    println!("╰──────────────────────────────────────────────────────────────────────────╯\n");
    println!("  ➜  Workspaces    : {workspace_line}");
    println!("  ➜  Default       : {}", hub.default_name());
    println!("  ➜  MCP Endpoint  : {}", config.mcp_url());
    println!(
        "  ➜  Health Check  : http://{}:{}/health",
        config.host, config.port
    );
    println!(
        "  ➜  Executor      : {} [{}] ({})",
        config.executor.kind, mode_str, config.executor.command
    );
    println!("  ➜  Auth          : {auth_str}");
    println!("  ➜  Allow Any Host: {allow_any_host}"); // 👈 单独成行打印配置值
    if let Some(pin) = oauth.generated_password() {
        println!("  ➜  Admin PIN     : {pin}\n     Enter this PIN at /oauth/authorize to approve ChatGPT / Gemini.");
    } else if require_auth && oauth.has_admin_password() {
        println!("  ➜  Admin PIN     : configured (.agentbridge.toml / --admin-password)");
    }
    if !require_auth && !loopback {
        println!("  ➜  WARNING       : public bind without auth");
    }
    println!("\n  ● Ready for Brain connections. Press Ctrl+C to stop.\n");
}
