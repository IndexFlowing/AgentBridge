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

use crate::config::Config;
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
    let mcp_url = config.mcp_url();
    let auth_token = config.auth_token.clone();
    let loopback = config.is_loopback();

    let ct = CancellationToken::new();
    let mut http_config = StreamableHttpServerConfig::default()
        .with_cancellation_token(ct.child_token())
        .with_json_response(true);

    if allow_any_host {
        http_config = http_config.disable_allowed_hosts();
        tracing::warn!(
            "Host header validation is disabled. Required for Cloudflare Tunnel; do not use on an untrusted network without auth_token."
        );
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
        tracing::info!("MCP endpoint requires Authorization: Bearer <token>");
    }

    let addr: SocketAddr = config
        .listen_addr()
        .parse()
        .with_context(|| format!("invalid listen address {}", config.listen_addr()))?;

    if !loopback {
        tracing::warn!(
            "Binding to {addr}. Remote clients can inspect the workspace and start the local OpenCode executor. Prefer 127.0.0.1 and a tunnel, and set auth_token."
        );
    }
    if allow_any_host && config.auth_token.is_none() {
        tracing::warn!(
            "Host header validation is disabled and no auth_token is set. \
             A remote Brain can start OpenCode on this machine. Set --auth-token."
        );
    }

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind {addr}"))?;

    tracing::info!("AgentBridge {}", env!("CARGO_PKG_VERSION"));
    tracing::info!("workspace {}", workspace.root().display());
    tracing::info!("MCP endpoint {mcp_url}");
    tracing::info!("health      http://{}:{}/health", config.host, config.port);
    tracing::info!(
        "Brain inspects via MCP. OpenCode runs only inside {} when task_start is called.",
        workspace.root().display()
    );

    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            ct.cancel();
        })
        .await?;
    Ok(())
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
