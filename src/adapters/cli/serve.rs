// src/cli/serve.rs
use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;

use crate::config;
use crate::core::AppCore;
use crate::server::{self, ServeOptions};

pub struct ServeArgs {
    pub config: Option<PathBuf>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub allow_any_host: bool,
    pub auth_token: Option<String>,
    pub no_auth: bool,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub admin_password: Option<String>,
}

pub fn load_serve_config(args: &ServeArgs) -> Result<(config::Config, PathBuf)> {
    if let Some(path) = args.config.as_deref() {
        let path = std::path::absolute(path)?;
        let cfg = config::Config::load_or_create(&path)?;
        Ok((cfg, path))
    } else {
        config::load_or_create_user_config()
    }
}

pub fn run(args: ServeArgs) -> Result<()> {
    // Startup-level settings come from specified --config or ~/.agentbridge/config.toml.
    // CLI flags and AGENTBRIDGE_* env vars are in-memory overrides.
    let (mut cfg, _config_path) = load_serve_config(&args)?;
    if let Some(host) = args.host {
        cfg.host = host;
    }
    if let Some(port) = args.port {
        cfg.port = port;
    }
    if args.allow_any_host {
        cfg.allow_any_host = true;
    }
    if let Some(token) = config::first_nonempty(
        args.auth_token,
        "AGENTBRIDGE_AUTH_TOKEN",
        cfg.auth_token.clone(),
    ) {
        cfg.auth_token = Some(token);
    }

    super::init_tracing(&cfg.logging.level);

    let no_auth = args.no_auth
        || cfg.no_auth
        || std::env::var("AGENTBRIDGE_NO_AUTH")
            .is_ok_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"));
    let options = ServeOptions {
        allow_any_host: cfg.allow_any_host,
        no_auth,
        client_id: config::first_nonempty(
            args.client_id,
            "AGENTBRIDGE_CLIENT_ID",
            cfg.client_id.clone(),
        ),
        client_secret: config::first_nonempty(
            args.client_secret,
            "AGENTBRIDGE_CLIENT_SECRET",
            cfg.client_secret.clone(),
        ),
        admin_password: config::first_nonempty(
            args.admin_password,
            "AGENTBRIDGE_ADMIN_PASSWORD",
            cfg.admin_password.clone(),
        ),
    };

    let core = Arc::new(AppCore::bootstrap(Arc::new(cfg))?);
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(server::serve(core, options))
}
