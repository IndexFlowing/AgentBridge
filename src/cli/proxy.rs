use std::path::PathBuf;
use anyhow::Result;
use clap::{Subcommand, ValueEnum};

use agentbridge::config::{self, ProxyKind};
use agentbridge::executor;

#[derive(Subcommand)]
pub enum ProxyCmd {
    /// Show proxy settings with credentials redacted
    Show { #[arg(long)] config: Option<PathBuf> },
    /// Update proxy settings in the AgentBridge config
    Set {
        #[arg(long)] config: Option<PathBuf>,
        #[arg(long)] kind: Option<ProxyKindArg>,
        #[arg(long)] host: Option<String>,
        #[arg(long)] port: Option<u16>,
        #[arg(long)] username: Option<String>,
        #[arg(long)] password: Option<String>,
        #[arg(long)] disable: bool,
    },
    /// Connect to example.com through the configured proxy
    Test { #[arg(long)] config: Option<PathBuf> },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ProxyKindArg { Http, Https, Socks5 }

impl From<ProxyKindArg> for ProxyKind {
    fn from(value: ProxyKindArg) -> Self {
        match value {
            ProxyKindArg::Http => Self::Http,
            ProxyKindArg::Https => Self::Https,
            ProxyKindArg::Socks5 => Self::Socks5,
        }
    }
}

pub fn run(command: ProxyCmd) -> Result<()> {
    match command {
        ProxyCmd::Show { config } => {
            let (cfg, path) = config::find_config(config.as_deref())?;
            println!("config  {}", path.display());
            println!("enabled {}", cfg.proxy.enabled);
            println!("kind    {:?}", cfg.proxy.kind);
            println!("host    {}", cfg.proxy.host);
            println!("port    {}", cfg.proxy.port);
            println!("auth    {}", if cfg.proxy.username.is_some() { "configured" } else { "none" });
            Ok(())
        }
        ProxyCmd::Set { config, kind, host, port, username, password, disable } => {
            let (mut cfg, path) = config::find_config(config.as_deref())?;
            cfg.proxy = config::apply_proxy_patch(
                &cfg.proxy,
                config::ProxyPatch {
                    enabled: Some(!disable),
                    kind: kind.map(Into::into),
                    host,
                    port,
                    username,
                    password,
                },
            )?;
            cfg.save_to_path(&path)?;
            println!("saved proxy settings to {}", path.display());
            Ok(())
        }
        ProxyCmd::Test { config } => {
            let (cfg, _) = config::find_config(config.as_deref())?;
            let rt = tokio::runtime::Runtime::new()?;
            println!("{}", rt.block_on(executor::test_proxy(&cfg.proxy))?);
            Ok(())
        }
    }
}