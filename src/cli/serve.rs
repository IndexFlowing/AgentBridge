use std::path::PathBuf;
use std::sync::Arc;
use anyhow::Result;

use agentbridge::config::{self, Config};
use agentbridge::projects::{self, ProjectHub};
use agentbridge::server::{self, ServeOptions};

pub struct ServeArgs {
    pub dir: Option<PathBuf>,
    pub config: Option<PathBuf>,
    pub workspaces: Option<PathBuf>,
    pub workspace: Option<PathBuf>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub allow_any_host: bool,
    pub auth_token: Option<String>,
    pub no_auth: bool,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub admin_password: Option<String>,
}

pub fn run(args: ServeArgs) -> Result<()> {
    let mut cfg = if let Some(path) = args.config.as_deref() {
        Config::load_from_path(path)?
    } else if args.dir.is_some() || args.workspaces.is_some() {
        match config::find_config(None) {
            Ok((c, _)) => c,
            Err(_) => {
                let path = args
                    .dir
                    .clone()
                    .or(args.workspace.clone())
                    .unwrap_or_else(|| std::env::current_dir().expect("cwd"));
                Config::new(std::path::absolute(path)?)
            }
        }
    } else {
        config::find_config(None)?.0
    };

    if let Some(host) = args.host {
        cfg.host = host;
    }
    if let Some(port) = args.port {
        cfg.port = port;
    }

    let (entries, default_name) = projects::discover(
        args.workspaces.as_deref(),
        args.dir.as_deref(),
        args.workspace.as_deref(),
        &cfg.workspace,
    )?;

    if let Some(first) = entries.first() {
        cfg.workspace = first.path.clone();
        if let Some(name) = &default_name {
            if let Some(found) = entries.iter().find(|e| &e.name == name) {
                cfg.workspace = found.path.clone();
            }
        }
    }

    if let Some(token) = config::first_nonempty(
        args.auth_token,
        "AGENTBRIDGE_AUTH_TOKEN",
        cfg.auth_token.clone(),
    ) {
        cfg.auth_token = Some(token);
    }

    let hub = ProjectHub::open(entries, default_name, Arc::new(cfg.clone()))?;

    let no_auth = args.no_auth
        || cfg.no_auth
        || std::env::var("AGENTBRIDGE_NO_AUTH").is_ok_and(|v| {
            matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes")
        });

    let options = ServeOptions {
        allow_any_host: args.allow_any_host || cfg.allow_any_host,
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

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(server::serve(cfg, hub, options))
}