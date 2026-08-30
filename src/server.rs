use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::extract::Request;
use axum::http::{header, HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use tokio_util::sync::CancellationToken;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::config::{Config, ExecutorMode};
use crate::mcp::AgentBridgeMcp;
use crate::task::TaskRuntime;
use crate::workspace::Workspace;

pub async fn serve(config: Config, allow_any_host: bool) -> Result<()> {
    let workspace = Workspace::open(
        &config.workspace,
        config.security.max_file_size,
        config.security.deny_sensitive_files,
    )
    .with_context(|| format!("cannot open workspace {}", config.workspace.display()))?;

    let workspace = Arc::new(workspace);
    let config = Arc::new(config);
    let runtime = Arc::new(
        TaskRuntime::new(workspace.clone(), config.clone())
            .context("invalid executor configuration")?,
    );
    let bind_host = config.host.clone();
    let bind_port = config.port;
    let auth_token = config.auth_token.clone();
    let loopback = config.is_loopback();

    let ct = CancellationToken::new();
    let mut http_config = StreamableHttpServerConfig::default()
        .with_cancellation_token(ct.child_token())
        .with_json_response(true);

    if allow_any_host {
        http_config = http_config.disable_allowed_hosts();
    } else {
        let mut hosts = vec![
            "localhost".to_string(),
            "127.0.0.1".to_string(),
            "[::1]".to_string(),
            format!("localhost:{bind_port}"),
            format!("127.0.0.1:{bind_port}"),
            format!("[::1]:{bind_port}"),
        ];
        if !hosts.iter().any(|h| h == &bind_host) {
            hosts.push(bind_host.clone());
            hosts.push(format!("{bind_host}:{bind_port}"));
        }
        http_config = http_config.with_allowed_hosts(hosts);
    }

    let ws = workspace.clone();
    let cfg = config.clone();
    let rt = runtime.clone();
    let service = StreamableHttpService::new(
        move || Ok(AgentBridgeMcp::new(ws.clone(), cfg.clone(), rt.clone())),
        LocalSessionManager::default().into(),
        http_config,
    );

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::any())
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers([
            header::CONTENT_TYPE,
            header::ACCEPT,
            header::AUTHORIZATION,
            HeaderName::from_static("mcp-protocol-version"),
            HeaderName::from_static("mcp-session-id"),
            HeaderName::from_static("last-event-id"),
        ])
        .expose_headers([HeaderName::from_static("mcp-session-id")]);

    let mut router = Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .nest_service("/mcp", service)
        .layer(cors);

    if let Some(token) = auth_token {
        router = router.layer(axum::middleware::from_fn(move |req, next| {
            let expected = token.clone();
            async move { require_bearer(expected, req, next).await }
        }));
    }

    let addr: SocketAddr = config
        .listen_addr()
        .parse()
        .with_context(|| format!("invalid listen address {}", config.listen_addr()))?;

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;

    // 优雅且正式的启动控制台面板
    print_startup_banner(&config, loopback, allow_any_host);

    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            ct.cancel();
        })
        .await?;
    Ok(())
}

fn print_startup_banner(config: &Config, loopback: bool, allow_any_host: bool) {
    let mode_str = match config.executor.mode {
        ExecutorMode::Stream => "stream (live terminal output)",
        ExecutorMode::Silent => "silent (quiet background)",
    };

    let auth_str = if config.auth_token.is_some() {
        "enabled (Bearer token required)"
    } else if loopback {
        "disabled (localhost mode)"
    } else {
        "disabled (WARN: public bind without auth)"
    };

    let file_limit_mb = (config.security.max_file_size as f64) / 1024.0 / 1024.0;
    let diff_limit_kb = config.security.max_diff_bytes / 1024;

    println!();
    println!("╭──────────────────────────────────────────────────────────────────────────╮");
    println!("│   AgentBridge v{:<7} — Autonomous Multi-Agent MCP Bridge                │", env!("CARGO_PKG_VERSION"));
    println!("╰──────────────────────────────────────────────────────────────────────────╯");
    println!();
    println!("  ➜  Workspace   : {}", config.workspace.display());
    println!("  ➜  MCP Endpoint: {} (Streamable HTTP)", config.mcp_url());
    println!("  ➜  Health Check: http://{}:{}/health", config.host, config.port);
    println!("  ➜  Executor    : {} [{}]", config.executor.kind, mode_str);
    println!("  ➜  Command     : {}", config.executor.command);
    println!("  ➜  Auth Token  : {}", auth_str);
    println!("  ➜  Security    : Read-only MCP sandbox | Max File: {:.1}MB | Max Diff: {}KB", file_limit_mb, diff_limit_kb);
    if allow_any_host {
        println!("  ➜  Host Check  : disabled (--allow-any-host for Cloudflare Tunnel)");
    }
    println!();
    println!("  ● Ready for Brain connections (Claude Desktop / Gemini / ChatGPT).");
    println!("  ● Press Ctrl+C to stop.");
    println!();
}

async fn root() -> &'static str {
    "AgentBridge — MCP workspace bridge (Brain inspects, OpenCode executes)\nMCP endpoint: /mcp\nHealth: /health\n"
}

async fn health() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        format!(
            "{{\"status\":\"ok\",\"name\":\"agentbridge\",\"version\":\"{}\"}}",
            env!("CARGO_PKG_VERSION")
        ),
    )
}

async fn require_bearer(expected: String, req: Request, next: Next) -> Response {
    let path = req.uri().path();
    if path == "/health" || path == "/" {
        return next.run(req).await;
    }
    let header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    let ok = header.is_some_and(|v| v.strip_prefix("Bearer ").is_some_and(|t| t == expected));
    if ok {
        next.run(req).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"))],
            "missing or invalid bearer token",
        )
            .into_response()
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut stream) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            stream.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}