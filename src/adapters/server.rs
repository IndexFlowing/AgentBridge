// src/server/mod.rs
pub mod banner;
pub mod middleware;
pub mod tunnel;
pub use tunnel::*;

use anyhow::{bail, Context, Result};
use axum::body::Body;
use axum::http::{header, HeaderName, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use rust_embed::RustEmbed;
use std::net::SocketAddr;
use std::sync::Arc;
use std::thread::JoinHandle;
use tokio_util::sync::CancellationToken;
use tower_http::cors::{AllowOrigin, CorsLayer};

use crate::api;
use crate::config::Config;
use crate::core::AppCore;
use crate::mcp::AgentBridgeMcp;
use crate::oauth::{self, AuthHttpState, OauthServer, OauthSettings};
use crate::storage::Storage;

pub use banner::print_startup_banner;
pub use middleware::mcp_diagnostics_layer;

// --- 静态文件打包配置 ---
#[derive(RustEmbed)]
#[folder = "web/"]
#[exclude = "node_modules/*"]
#[exclude = "src/*"]
#[exclude = "*.json"]
#[exclude = "*.mjs"]
struct WebAssets;

async fn static_handler(uri: Uri) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/');
    if path.is_empty() {
        path = "index.html";
    }

    match WebAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data))
                .unwrap()
        }
        None => {
            if let Some(index) = WebAssets::get("index.html") {
                Response::builder()
                    .header(header::CONTENT_TYPE, "text/html")
                    .body(Body::from(index.data))
                    .unwrap()
            } else {
                (StatusCode::NOT_FOUND, "404 Not Found").into_response()
            }
        }
    }
}

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

pub async fn serve(core: Arc<AppCore>, options: ServeOptions) -> Result<()> {
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        stop.cancel();
    });
    serve_with_cancel(core, options, cancel, true).await?;
    Ok(())
}

pub fn spawn_server(core: Arc<AppCore>, options: ServeOptions) -> Result<ServeHandle> {
    let (oauth, require_auth) = build_oauth(&core.config, &options, core.storage.clone());
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
                    core,
                    options,
                    oauth,
                    cancel,
                    require_auth,
                    true,
                    Some(ready_tx),
                ))
            }
        })?;
    match ready_rx.recv() {
        Ok(Ok(())) => Ok(ServeHandle {
            oauth,
            cancel,
            thread: Some(thread),
        }),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => {
            let _ = thread.join();
            bail!("HTTP thread exited early");
        }
    }
}

async fn serve_with_cancel(
    core: Arc<AppCore>,
    options: ServeOptions,
    cancel: CancellationToken,
    banner: bool,
) -> Result<Arc<OauthServer>> {
    let (oauth, require_auth) = build_oauth(&core.config, &options, core.storage.clone());
    let oauth = Arc::new(oauth);
    run_http(
        core,
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
    core: Arc<AppCore>,
    options: ServeOptions,
    oauth: Arc<OauthServer>,
    cancel: CancellationToken,
    require_auth: bool,
    banner: bool,
    ready: Option<std::sync::mpsc::Sender<Result<()>>>,
) -> Result<()> {
    let router = build_router(core.clone(), oauth.clone(), options.allow_any_host);

    let addr: SocketAddr = core
        .config
        .listen_addr()
        .parse()
        .with_context(|| format!("invalid listen address {}", core.config.listen_addr()))?;
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(err) => {
            let e = anyhow::Error::new(err).context(format!("failed to bind {addr}"));
            if let Some(tx) = ready {
                let _ = tx.send(Err(anyhow::Error::msg(e.to_string())));
            }
            return Err(e);
        }
    };

    if let Some(tx) = ready {
        let _ = tx.send(Ok(()));
    }
    if banner {
        print_startup_banner(
            &core.config,
            &core.hub,
            &oauth,
            core.config.is_loopback(),
            options.allow_any_host,
            require_auth,
            options.no_auth,
        );
    }

    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            cancel.cancelled().await;
        })
        .await?;
    Ok(())
}

pub fn build_router(core: Arc<AppCore>, oauth: Arc<OauthServer>, allow_any_host: bool) -> Router {
    let bind_host = core.config.host.clone();
    let bind_port = core.config.port;
    let listen_base = format!("http://{bind_host}:{bind_port}");

    let auth_state = AuthHttpState {
        oauth: oauth.clone(),
        listen_base,
    };
    let mut http_config = StreamableHttpServerConfig::default().with_json_response(true);
    if allow_any_host {
        http_config = http_config.disable_allowed_hosts();
    }

    let mcp_core = core.clone();
    let service = StreamableHttpService::new(
        move || Ok(AgentBridgeMcp::new(mcp_core.clone())),
        LocalSessionManager::default().into(),
        http_config,
    );

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::any())
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::DELETE,
            Method::OPTIONS,
            Method::PUT,
        ])
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
        .layer(axum::middleware::from_fn(mcp_diagnostics_layer))
        .layer(axum::middleware::from_fn_with_state(
            auth_state.clone(),
            oauth::mcp_auth_middleware,
        ));

    let api_state = api::ApiState { core, oauth };

    oauth::router()
        .route("/health", get(health))
        .with_state(auth_state)
        .merge(mcp)
        .nest("/api", api::router(api_state))
        .layer(cors)
        .fallback(static_handler)
}

pub fn build_oauth(
    config: &Config,
    options: &ServeOptions,
    db: Arc<Storage>,
) -> (OauthServer, bool) {
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
    (
        OauthServer::new(
            OauthSettings {
                require_auth,
                admin_password,
                password_generated,
                static_token: config.auth_token.clone(),
                client_id: options.client_id.clone(),
                client_secret: options.client_secret.clone(),
            },
            db,
        ),
        require_auth,
    )
}

async fn health() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        format!(
            "{{\"status\":\"ok\",\"version\":\"{}\"}}",
            env!("CARGO_PKG_VERSION")
        ),
    )
}
async fn shutdown_signal() {
    // Ctrl-C on every platform; also SIGTERM on Unix so `agentbridge stop`
    // can request a graceful shutdown instead of a hard kill.
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut terminate) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = terminate.recv() => {}
                }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
