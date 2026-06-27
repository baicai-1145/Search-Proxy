use crate::config::{Config, WrapConfig};
use crate::provider::Provider;
use anyhow::{anyhow, bail, Result};
use serde::Deserialize;
use std::process::{Command, Stdio};

/// Markers (case-insensitive) in the real CLI's stderr/stdout that indicate a
/// rate-limit / quota error worth rotating the key for.
const LIMIT_MARKERS: &[&str] = &[
    "402",
    "429",
    "rate limit",
    "rate-limit",
    "ratelimit",
    "quota",
    "credits",
    "payment required",
    "too many requests",
];

/// Mode A: lease a key from the VPS pool, inject it into the real CLI's env,
/// and exec the real CLI directly against the real API (large traffic stays
/// local). On a rate-limit/quota exit, report + re-lease + retry (up to 3).
pub async fn wrap(provider_str: &str, args: &[String]) -> Result<()> {
    let provider = Provider::from_str(provider_str)
        .ok_or_else(|| anyhow!("unknown provider: {provider_str}"))?;
    let cfg = Config::load_default()?;
    let vps_url = cfg
        .wrap
        .vps_url
        .as_deref()
        .ok_or_else(|| anyhow!("wrap.vps_url not set in config"))?;
    let token = cfg
        .wrap
        .control_token
        .as_deref()
        .ok_or_else(|| anyhow!("wrap.control_token not set in config"))?;
    let real_cli = resolve_real_cli(provider, &cfg.wrap)?;
    let base = vps_url.trim_end_matches('/').to_string();
    let client = reqwest::Client::new();

    const MAX_ATTEMPTS: usize = 3;
    for attempt in 1..=MAX_ATTEMPTS {
        let lease = match lease_key(&client, &base, token, provider).await {
            Ok(l) => l,
            Err(e) => {
                if attempt == 1 {
                    return Err(e);
                }
                tracing::warn!(attempt, error = %e, "lease failed on retry; stopping");
                bail!("no available key (lease failed: {e})");
            }
        };

        let output = Command::new(&real_cli)
            .args(args)
            .env(provider.api_key_env(), &lease.key)
            .env(provider.base_url_env(), provider.real_base_url())
            .stdin(Stdio::inherit())
            .output()?;

        // Pass the real CLI's output through to our caller.
        use std::io::Write;
        std::io::stdout().write_all(&output.stdout).ok();
        std::io::stderr().write_all(&output.stderr).ok();

        if output.status.success() {
            let _ = report(&client, &base, token, lease.key_id, "ok", None).await;
            return Ok(());
        }

        let combined = format!(
            "{} {}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
        let lower = combined.to_ascii_lowercase();
        let is_limit = LIMIT_MARKERS.iter().any(|m| lower.contains(m));

        if !is_limit {
            let code = output.status.code().unwrap_or(1);
            // Non-limit failure: don't burn another key. Don't report a limit
            // state, just surface the real CLI's exit.
            bail!(
                "real CLI `{}` exited {code} (non-limit error; not retrying)",
                provider.cli_bin()
            );
        }

        let result = if lower.contains("402")
            || lower.contains("quota")
            || lower.contains("payment required")
            || lower.contains("credits")
        {
            "402"
        } else {
            "429"
        };
        tracing::warn!(%provider, key_id = lease.key_id, attempt, result, "key hit limit; reporting + rotating");
        let _ = report(&client, &base, token, lease.key_id, result, None).await;
    }
    bail!("exhausted {MAX_ATTEMPTS} attempts; all keys may be cooling down")
}

#[derive(Deserialize)]
struct LeaseResp {
    key: String,
    key_id: i64,
    #[allow(dead_code)]
    lease_id: String,
    #[allow(dead_code)]
    provider: String,
}

async fn lease_key(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    provider: Provider,
) -> Result<LeaseResp> {
    let resp = client
        .get(format!("{base}/lease"))
        .query(&[("provider", &provider.to_string())])
        .bearer_auth(token)
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("lease failed (HTTP {status}): {body}");
    }
    let lease = resp.json::<LeaseResp>().await?;
    Ok(lease)
}

async fn report(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    key_id: i64,
    result: &str,
    retry_after: Option<i64>,
) -> Result<()> {
    client
        .post(format!("{base}/report"))
        .bearer_auth(token)
        .json(&serde_json::json!({"key_id": key_id, "result": result, "retry_after": retry_after}))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

fn resolve_real_cli(provider: Provider, cfg: &WrapConfig) -> Result<String> {
    if let Some(p) = match provider {
        Provider::Firecrawl => cfg.cli_firecrawl_path.as_deref(),
        Provider::Tavily => cfg.cli_tvly_path.as_deref(),
    } {
        return Ok(p.to_string());
    }
    let out = Command::new("which").arg(provider.cli_bin()).output()?;
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if path.is_empty() {
        bail!(
            "could not find real `{}` binary; set wrap.cli_{}_path in config or run `search-proxy install`",
            provider.cli_bin(),
            provider
        );
    }
    Ok(path)
}
