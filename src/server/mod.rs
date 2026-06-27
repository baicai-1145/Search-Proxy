mod control;
#[allow(dead_code)]
mod proxy;
#[allow(dead_code)]
mod webui_api;
mod webui_static;

use crate::config::Config;
use crate::keypool::KeyPool;
use crate::server::control::{control_router, AppState};
use anyhow::Result;
use std::net::SocketAddr;

/// Entry point for `search-proxy serve`. Loads config, opens the key pool, and
/// serves the control plane (`/lease`, `/report`). The reverse proxy and web UI
/// are mounted in M2 and M5.
pub async fn serve() -> Result<()> {
    let cfg = Config::load_default()?;
    if cfg.server.control_token.is_none() {
        anyhow::bail!("server.control_token is required for `serve`");
    }
    let pool = KeyPool::open(&cfg.database.path).await?;
    // Background active-usage refresher (firecrawl credit queries; tavily skipped).
    {
        let rp = pool.clone();
        let rc = cfg.clone();
        let client = reqwest::Client::new();
        tokio::spawn(async move {
            let secs = rc.rotation.usage_refresh_secs.max(1) as u64;
            loop {
                if let Err(e) = rp
                    .refresh_usage(
                        &client,
                        rc.rotation.rate_limit_cooldown_secs,
                        rc.rotation.quota_cooldown_secs,
                    )
                    .await
                {
                    tracing::warn!(error = %e, "background usage refresh error");
                }
                tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
            }
        });
    }
    let state = AppState {
        pool,
        cfg: cfg.clone(),
    };
    let app = control_router(state);
    let addr: SocketAddr = cfg.server.listen.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(
        listen = %cfg.server.listen,
        "search-proxy control plane listening (lease/report); data plane + webui in M2/M5"
    );
    axum::serve(listener, app).await?;
    Ok(())
}
