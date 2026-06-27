use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub database: DbConfig,
    #[serde(default)]
    pub rotation: RotationConfig,
    #[serde(default)]
    pub wrap: WrapConfig,
    #[serde(default)]
    pub admin: AdminConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_listen")]
    pub listen: String,
    /// Control-plane token used by the mode-A wrapper for /lease and /report.
    /// Required for `serve`; not needed for `wrap`-only configs.
    #[serde(default)]
    pub control_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DbConfig {
    #[serde(default = "default_db_path")]
    pub path: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RotationConfig {
    #[serde(default = "default_rl_cooldown")]
    pub rate_limit_cooldown_secs: i64,
    #[serde(default = "default_quota_cooldown")]
    pub quota_cooldown_secs: i64,
    #[serde(default = "default_refresh")]
    pub usage_refresh_secs: i64,
}

/// Mode-A (wrapper) configuration: where the VPS control plane is and how to
/// reach it, plus optional absolute paths to the real CLIs (normally filled by
/// `search-proxy install`).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct WrapConfig {
    pub vps_url: Option<String>,
    pub control_token: Option<String>,
    pub cli_firecrawl_path: Option<String>,
    pub cli_tvly_path: Option<String>,
}

/// Admin/WebUI configuration. If `password` is unset, admin login is disabled.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct AdminConfig {
    pub password: Option<String>,
    #[serde(default = "default_admin_ttl")]
    pub session_ttl_secs: i64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            listen: default_listen(),
            control_token: None,
        }
    }
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            path: default_db_path(),
        }
    }
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            rate_limit_cooldown_secs: default_rl_cooldown(),
            quota_cooldown_secs: default_quota_cooldown(),
            usage_refresh_secs: default_refresh(),
        }
    }
}

fn default_listen() -> String {
    "127.0.0.1:8787".into()
}
fn default_db_path() -> PathBuf {
    PathBuf::from("data/search-proxy.db")
}
fn default_rl_cooldown() -> i64 {
    60
}
fn default_quota_cooldown() -> i64 {
    6 * 3600
}
fn default_refresh() -> i64 {
    600
}
fn default_admin_ttl() -> i64 {
    7 * 86400
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("read config {}", path.display()))?;
        let cfg: Config = toml::from_str(&text).context("parse config")?;
        Ok(cfg)
    }

    /// Load from `search-proxy.toml` (CWD) or `~/.config/search-proxy/config.toml`.
    pub fn load_default() -> Result<Self> {
        let candidates: [Option<PathBuf>; 2] = [
            Some(PathBuf::from("search-proxy.toml")),
            std::env::var_os("HOME").map(|h| {
                let mut p = PathBuf::from(h);
                p.push(".config/search-proxy/config.toml");
                p
            }),
        ];
        for c in candidates.into_iter().flatten() {
            if c.exists() {
                return Self::load(&c);
            }
        }
        anyhow::bail!(
            "no config file found (looked for ./search-proxy.toml and ~/.config/search-proxy/config.toml)"
        )
    }
}
