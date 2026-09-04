use std::net::SocketAddr;
use std::sync::Arc;
use std::thread::JoinHandle;

use anyhow::{bail, Context, Result};
use axum::http::{header, HeaderName, Method, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use tokio_util::sync::CancellationToken;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::config::{Config, ExecutorMode};
use crate::mcp::AgentBridgeMcp;
use crate::oauth::{self, AuthHttpState, OauthServer, OauthSettings};
use crate::projects::ProjectHub;

pub struct ServeOptions {
    pub allow_any_host: bool,
    pub no_auth: bool,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub admin_password: Option<String>,
}

pub struct ServeHandle {
    pub oauth: Arc<OauthServer>,
    cancel: CancellationToken,
    thread: Option<JoinHandle<Result<()>>>,
}

impl ServeHandle {
    pub fn stop(&mut self) {
        self.cancel.cancel();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for ServeHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

pub async fn serve(config: Config, hub: ProjectHub, options: ServeOptions) -> Result<()> {
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        stop.cancel();
    });
    serve_with_cancel(config, hub, options, cancel, true).await?;
    Ok(())
}

/// Bind and run the MCP server until `cancel` is triggered. Used by the tray UI.
pub fn spawn_server(
    config: Config,
    hub: ProjectHub,
    options: ServeOptions,
) -> Result<ServeHandle> {
    let (oauth, require_auth) = build_oauth(&config, &options);
    let oauth = Arc::new(oauth);
    let cancel = CancellationToken::new();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let thread = std::thread::Builder::new()
        .name("agentbridge-http".into())
        .spawn({
            let oauth = oauth.clone();
            let cancel = cancel.clone();
            move || {
                let rt = tokio::runtime::Runtime::new()?;
                rt.block_on(run_http(
                    config,
                    hub,
                    options,
                    oauth,
                    cancel,
                    require_auth,
                    true,
                    Some(ready_tx),
                ))
            }
        })
        .context("failed to start HTTP thread")?;
    match ready_rx.recv() {
        Ok(Ok(())) => Ok(ServeHandle {
            oauth,
            cancel,
            thread: Some(thread),
        }),
        Ok(Err(err)) => {
            let _ = thread.join();
            Err(err)
        }
        Err(_) => {
            let _ = thread.join();
            bail!("HTTP thread exited before binding");
        }
    }
}

async fn serve_with_cancel(
    config: Config,
    hub: ProjectHub,
    options: ServeOptions,
    cancel: CancellationToken,
    banner: bool,
) -> Result<Arc<OauthServer>> {
    let (oauth, require_auth) = build_oauth(&config, &options);
    let oauth = Arc::new(oauth);
    run_http(
        config,
        hub,
        options,
        oauth.clone(),
        cancel,
        require_auth,
        banner,
        None,
    )
    .await?;
    Ok(oauth)
}

#[allow(clippy::too_many_arguments)]
async fn run_http(
    config: Config,
    hub: ProjectHub,
    options: ServeOptions,
    oauth: Arc<OauthServer>,
    cancel: CancellationToken,
    require_auth: bool,
    banner: bool,
    ready: Option<std::sync::mpsc::Sender<Result<()>>>,
) -> Result<()> {
    let config = Arc::new(config);
    let hub = Arc::new(hub);
    let bind_host = config.host.clone();
    let bind_port = config.port;
    let loopback = config.is_loopback();

    let listen_base = if config.host == "0.0.0.0" || config.host == "::" {
        format!("http://127.0.0.1:{bind_port}")
    } else {
        format!("http://{}:{bind_port}", config.host)
    };

    let auth_state = AuthHttpState {
        oauth: oauth.clone(),
        listen_base: listen_base.clone(),
    };

    let mcp_ct = CancellationToken::new();
    let mut http_config = StreamableHttpServerConfig::default()
        .with_cancellation_token(mcp_ct.child_token())
        .with_json_response(true);

    if options.allow_any_host {
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

    let ws_hub = hub.clone();
    let cfg = config.clone();
    let service = StreamableHttpService::new(
        move || Ok(AgentBridgeMcp::new(ws_hub.clone(), cfg.clone())),
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
        .expose_headers([
            HeaderName::from_static("mcp-session-id"),
            header::WWW_AUTHENTICATE,
        ]);

    let mcp = Router::new()
        .nest_service("/mcp", service)
        .layer(axum::middleware::from_fn_with_state(
            auth_state.clone(),
            oauth::mcp_auth_middleware,
        ));

    let public = oauth::router()
        .route("/", get(root))
        .route("/health", get(health))
        .with_state(auth_state);

    let router = public.merge(mcp).layer(cors);

    let addr: SocketAddr = config
        .listen_addr()
        .parse()
        .with_context(|| format!("invalid listen address {}", config.listen_addr()))?;

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(err) => {
            let wrapped = anyhow::Error::new(err)
                .context(format!("failed to bind {addr}"));
            if let Some(tx) = ready {
                let _ = tx.send(Err(anyhow::Error::msg(wrapped.to_string())));
            }
            return Err(wrapped);
        }
    };

    if let Some(tx) = ready {
        let _ = tx.send(Ok(()));
    }

    if banner {
        print_startup_banner(
            &config,
            &hub,
            &oauth,
            loopback,
            options.allow_any_host,
            require_auth,
            options.no_auth,
        );
    }

    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            cancel.cancelled().await;
            mcp_ct.cancel();
        })
        .await?;
    Ok(())
}

/// Build the HTTP router (public OAuth + protected `/mcp`). Used by tests.
pub fn build_router(
    config: Arc<Config>,
    hub: Arc<ProjectHub>,
    oauth: Arc<OauthServer>,
    allow_any_host: bool,
) -> Router {
    let bind_host = config.host.clone();
    let bind_port = config.port;
    let listen_base = format!("http://{}:{bind_port}", config.host);

    let auth_state = AuthHttpState {
        oauth: oauth.clone(),
        listen_base,
    };

    let mut http_config = StreamableHttpServerConfig::default().with_json_response(true);
    if allow_any_host {
        http_config = http_config.disable_allowed_hosts();
    } else {
        http_config = http_config.with_allowed_hosts(vec![
            "localhost".into(),
            "127.0.0.1".into(),
            format!("localhost:{bind_port}"),
            format!("127.0.0.1:{bind_port}"),
            bind_host,
        ]);
    }

    let ws_hub = hub.clone();
    let cfg = config.clone();
    let service = StreamableHttpService::new(
        move || Ok(AgentBridgeMcp::new(ws_hub.clone(), cfg.clone())),
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
        .expose_headers([
            HeaderName::from_static("mcp-session-id"),
            header::WWW_AUTHENTICATE,
        ]);

    let mcp = Router::new()
        .nest_service("/mcp", service)
        .layer(axum::middleware::from_fn_with_state(
            auth_state.clone(),
            oauth::mcp_auth_middleware,
        ));

    oauth::router()
        .route("/", get(root))
        .route("/health", get(health))
        .with_state(auth_state)
        .merge(mcp)
        .layer(cors)
}

pub fn build_oauth(config: &Config, options: &ServeOptions) -> (OauthServer, bool) {
    let require_auth = !options.no_auth;
    let (admin_password, password_generated) = match options
        .admin_password
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(p) => (p.to_string(), false),
        None if require_auth => (oauth::generate_admin_password(), true),
        None => (String::new(), false),
    };
    let settings = OauthSettings {
        require_auth,
        admin_password,
        password_generated,
        static_token: config.auth_token.clone(),
        client_id: options.client_id.clone(),
        client_secret: options.client_secret.clone(),
    };
    (OauthServer::new(settings), require_auth)
}

fn print_startup_banner(
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

    let file_limit_mb = (config.security.max_file_size as f64) / 1024.0 / 1024.0;
    let diff_limit_kb = config.security.max_diff_bytes / 1024;

    println!();
    println!("╭──────────────────────────────────────────────────────────────────────────╮");
    println!(
        "│   AgentBridge v{:<7} — Autonomous Multi-Agent MCP Bridge                │",
        env!("CARGO_PKG_VERSION")
    );
    println!("╰──────────────────────────────────────────────────────────────────────────╯");
    println!();
    println!("  ➜  Workspaces  : {workspace_line}");
    println!("  ➜  Default     : {}", hub.default_name());
    println!("  ➜  MCP Endpoint: {} (Streamable HTTP)", config.mcp_url());
    println!(
        "  ➜  Health Check: http://{}:{}/health",
        config.host, config.port
    );
    println!("  ➜  Executor    : {} [{}]", config.executor.kind, mode_str);
    println!("  ➜  Command     : {}", config.executor.command);
    println!("  ➜  Auth        : {auth_str}");
    if let Some(pin) = oauth.generated_password() {
        println!("  ➜  Admin PIN   : {pin}");
        println!("     Enter this PIN at /oauth/authorize to approve ChatGPT / Gemini.");
    } else if require_auth && oauth.has_admin_password() {
        println!("  ➜  Admin PIN   : configured (.agentbridge.toml / --admin-password)");
    }
    println!(
        "  ➜  Security    : Per-project sandbox | Max File: {:.1}MB | Max Diff: {}KB",
        file_limit_mb, diff_limit_kb
    );
    if allow_any_host {
        println!("  ➜  Host Check  : disabled (--allow-any-host for Cloudflare Tunnel)");
    }
    if !require_auth && !loopback {
        println!("  ➜  WARNING     : public bind without auth");
    }
    println!();
    println!("  ● Ready for Brain connections (Claude Desktop / Gemini / ChatGPT).");
    println!("  ● Press Ctrl+C to stop.");
    println!();
}

async fn root() -> &'static str {
    "AgentBridge — MCP workspace bridge (Brain inspects, OpenCode executes)\n\
     MCP endpoint: /mcp\n\
     Health: /health\n\
     OAuth authorize: /oauth/authorize\n\
     OAuth token: /oauth/token\n"
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
