use crate::config::Config;
use crate::keypool::KeyPool;
use crate::provider::Provider;
use anyhow::{anyhow, bail, Context, Result};
use clap::Subcommand;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Subcommand, Debug)]
pub enum KeyAction {
    /// Add an API key to the pool
    Add {
        /// Provider: firecrawl | tavily
        provider: String,
        /// The API key value (firecrawl: fc-..., tavily: tvly-...)
        key: String,
        /// Optional account/team tag (firecrawl limits are per-team; same-team
        /// keys don't help rotation)
        #[arg(long)]
        account: Option<String>,
    },
    /// List all keys and their status
    List,
    /// Remove a key by id
    Remove { id: i64 },
}

#[derive(Subcommand, Debug)]
pub enum UserAction {
    /// Create a user token (mode B)
    Create {
        #[arg(long)]
        name: Option<String>,
    },
    /// List users
    List,
    /// Revoke a user token
    Revoke { token: String },
}

pub async fn run_key(action: KeyAction) -> Result<()> {
    let cfg = Config::load_default()?;
    let pool = KeyPool::open(&cfg.database.path).await?;
    match action {
        KeyAction::Add {
            provider,
            key,
            account,
        } => {
            let p = Provider::from_str(&provider)
                .ok_or_else(|| anyhow!("unknown provider: {provider}"))?;
            pool.add_key(p, &key, account.as_deref()).await?;
            println!("added {p} key");
        }
        KeyAction::List => {
            let rows = pool.list_keys().await?;
            if rows.is_empty() {
                println!("(no keys)");
                return Ok(());
            }
            println!(
                "{:<4} {:<10} {:<14} {:<14} {:<16} {:<8} {}",
                "id", "provider", "account", "status", "cooldown_until", "credits", "key"
            );
            for r in rows {
                let cd = r
                    .cooldown_until
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "-".into());
                let cr = r
                    .credits_remaining
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "-".into());
                let acc = r.account_team.unwrap_or_else(|| "-".into());
                println!(
                    "{:<4} {:<10} {:<14} {:<14} {:<16} {:<8} {}",
                    r.id,
                    r.provider,
                    acc,
                    r.status,
                    cd,
                    cr,
                    mask_key(&r.key_value)
                );
            }
        }
        KeyAction::Remove { id } => {
            pool.remove_key(id).await?;
            println!("removed key {id}");
        }
    }
    Ok(())
}

/// Show only head/tail of a key to avoid pasting full secrets into terminals.
fn mask_key(k: &str) -> String {
    let len = k.chars().count();
    if len <= 10 {
        k.to_string()
    } else {
        let head: String = k.chars().take(6).collect();
        let tail: String = k.chars().skip(len - 4).collect();
        format!("{head}...{tail}")
    }
}

pub async fn run_user(action: UserAction) -> Result<()> {
    let cfg = Config::load_default()?;
    let pool = KeyPool::open(&cfg.database.path).await?;
    match action {
        UserAction::Create { name } => {
            let token = pool.create_user(name.as_deref()).await?;
            println!("created user token: {token}");
            println!("point the CLI at the VPS and use this token as the API key:");
            println!("  firecrawl --api-url https://<vps>/firecrawl   (FIRECRAWL_API_KEY={token})");
            println!("  TAVILY_API_BASE_URL=https://<vps>/tavily      TAVILY_API_KEY={token}");
        }
        UserAction::List => {
            let users = pool.list_users().await?;
            if users.is_empty() {
                println!("(no users)");
                return Ok(());
            }
            println!(
                "{:<4} {:<36} {:<16} {}",
                "id", "token", "name", "created_at"
            );
            for u in users {
                println!(
                    "{:<4} {:<36} {:<16} {}",
                    u.id,
                    u.token,
                    u.name.unwrap_or_else(|| "-".into()),
                    u.created_at
                );
            }
        }
        UserAction::Revoke { token } => {
            pool.revoke_user(&token).await?;
            println!("revoked {token}");
        }
    }
    Ok(())
}

/// Mode A setup: probe the real `firecrawl`/`tvly` binaries, write PATH shims
/// that exec `search-proxy wrap <provider>`, and record the real paths in the
/// wrap config so `wrap` can bypass the shims.
pub async fn install(dir: Option<&str>) -> Result<()> {
    let dir = dir.map(PathBuf::from).unwrap_or_else(default_install_dir);
    tokio::fs::create_dir_all(&dir)
        .await
        .with_context(|| format!("create shim dir {}", dir.display()))?;
    let self_exe = std::env::current_exe().context("locate current executable")?;
    let firecrawl = probe_real_cli("firecrawl", &dir)?;
    let tvly = probe_real_cli("tvly", &dir)?;
    write_shim(&dir.join("firecrawl"), &self_exe, "firecrawl")?;
    write_shim(&dir.join("tvly"), &self_exe, "tvly")?;
    update_wrap_paths(&firecrawl, &tvly)?;
    println!("installed shims to {}", dir.display());
    println!("real firecrawl -> {}", firecrawl.display());
    println!("real tvly      -> {}", tvly.display());
    if !in_path(&dir) {
        println!("NOTE: add {} to your PATH", dir.display());
    }
    Ok(())
}

fn default_install_dir() -> PathBuf {
    let mut p = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    p.push(".local/bin");
    p
}

/// Find the real CLI binary, skipping any shim already installed in `shim_dir`.
fn probe_real_cli(bin: &str, shim_dir: &Path) -> Result<PathBuf> {
    let out = Command::new("which")
        .arg("-a")
        .arg(bin)
        .output()
        .with_context(|| format!("run `which -a {bin}`"))?;
    let lines = String::from_utf8_lossy(&out.stdout);
    for line in lines.lines() {
        let p = PathBuf::from(line.trim());
        if p.as_os_str().is_empty() || p.starts_with(shim_dir) {
            continue;
        }
        if p.exists() {
            return Ok(p);
        }
    }
    bail!("could not find real `{bin}` binary in PATH (excluding shim dir)");
}

fn write_shim(path: &Path, self_exe: &Path, provider: &str) -> Result<()> {
    let content = format!(
        "#!/bin/sh\nexec \"{}\" wrap {} -- \"$@\"\n",
        self_exe.display(),
        provider
    );
    std::fs::write(path, content).with_context(|| format!("write shim {}", path.display()))?;
    make_executable(path)?;
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(windows)]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

/// Record real CLI paths into the wrap config (`~/.config/search-proxy/config.toml`),
/// creating or merging the file without touching other sections.
fn update_wrap_paths(firecrawl: &Path, tvly: &Path) -> Result<()> {
    let cfg_path = wrap_config_path();
    if let Some(parent) = cfg_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let mut doc: toml::Value = if cfg_path.exists() {
        std::fs::read_to_string(&cfg_path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_else(|| toml::Value::Table(Default::default()))
    } else {
        toml::Value::Table(Default::default())
    };
    let table = doc
        .as_table_mut()
        .ok_or_else(|| anyhow!("config root is not a table"))?;
    let wrap = table
        .entry("wrap")
        .or_insert_with(|| toml::Value::Table(Default::default()));
    if let Some(t) = wrap.as_table_mut() {
        t.insert(
            "cli_firecrawl_path".into(),
            toml::Value::String(firecrawl.display().to_string()),
        );
        t.insert(
            "cli_tvly_path".into(),
            toml::Value::String(tvly.display().to_string()),
        );
    }
    std::fs::write(&cfg_path, toml::to_string_pretty(&doc)?)?;
    println!("updated {}", cfg_path.display());
    Ok(())
}

fn wrap_config_path() -> PathBuf {
    let mut p = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
    p.push(".config/search-proxy/config.toml");
    p
}

fn in_path(dir: &Path) -> bool {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).any(|entry| entry == dir))
        .unwrap_or(false)
}

pub async fn status() -> Result<()> {
    tracing::info!("status (stub; M1 will query pool + usage)");
    Ok(())
}
