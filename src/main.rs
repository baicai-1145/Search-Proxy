#![allow(dead_code)] // M0 skeleton; revisit as M1+ wires these up

use anyhow::Result;
use clap::{Parser, Subcommand};

mod auth;
mod cli_admin;
mod config;
mod keypool;
mod provider;
mod server;
mod wrap;

#[derive(Parser)]
#[command(
    name = "search-proxy",
    version,
    about = "Multi-account key-rotation proxy for Firecrawl and Tavily CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the server on VPS: control plane + reverse proxy + webui
    Serve,
    /// Mode A wrapper: lease a key, inject env, exec the real CLI
    Wrap {
        /// Provider: firecrawl | tavily
        provider: String,
        /// Args passed through verbatim to the real CLI (including flags like
        /// `--limit`); `trailing_var_arg` stops clap from parsing them.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Manage API keys in the pool
    Key {
        #[command(subcommand)]
        action: cli_admin::KeyAction,
    },
    /// Manage user tokens (mode B)
    User {
        #[command(subcommand)]
        action: cli_admin::UserAction,
    },
    /// Install PATH shims so `firecrawl`/`tvly` go through `search-proxy wrap`
    Install {
        /// Shim install directory (default: ~/.local/bin)
        #[arg(long)]
        dir: Option<String>,
    },
    /// Show pool and usage status
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Serve => server::serve().await,
        Command::Wrap { provider, args } => wrap::wrap(&provider, &args).await,
        Command::Key { action } => cli_admin::run_key(action).await,
        Command::User { action } => cli_admin::run_user(action).await,
        Command::Install { dir } => cli_admin::install(dir.as_deref()).await,
        Command::Status => cli_admin::status().await,
    }
}
