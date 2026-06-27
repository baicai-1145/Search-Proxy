use crate::provider::Provider;
use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{FromRow, SqlitePool};
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use time::OffsetDateTime;

#[derive(Clone)]
pub struct KeyPool {
    pub db: SqlitePool,
}

/// A leased key handed to a mode-A wrapper or mode-B proxy worker.
pub struct Lease {
    pub key_id: i64,
    pub key_value: String,
    pub lease_id: String,
    pub provider: Provider,
}

#[derive(Debug, FromRow)]
pub struct KeyRow {
    pub id: i64,
    pub provider: String,
    pub account_team: Option<String>,
    pub key_value: String,
    pub status: String,
    pub cooldown_until: Option<i64>,
    pub credits_remaining: Option<i64>,
    pub last_used_at: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct UserRow {
    pub id: i64,
    pub token: String,
    pub name: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PoolStats {
    pub keys_by_status: HashMap<String, i64>,
    pub total_keys: i64,
    pub total_users: i64,
}

impl KeyPool {
    pub async fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await.ok();
            }
        }
        let url = format!("sqlite://{}", path.display());
        let opts = SqliteConnectOptions::from_str(&url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5));
        let db = SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(opts)
            .await?;
        sqlx::migrate!("./migrations")
            .run(&db)
            .await
            .context("database migration failed")?;
        Ok(KeyPool { db })
    }

    /// Pick the least-recently-used available key for `provider`. A key whose
    /// cooldown has expired is transparently reactivated here so we don't need
    /// a background sweeper for correctness (M3 refresh keeps display accurate).
    pub async fn lease(&self, provider: Provider) -> Result<Lease> {
        let now = now_ts();
        let row: Option<(i64, String, String)> = sqlx::query_as(
            "SELECT id, key_value, status FROM keys \
             WHERE provider = ?1 AND status NOT IN ('auth-failed','disabled') \
             AND (cooldown_until IS NULL OR cooldown_until < ?2) \
             ORDER BY last_used_at ASC, id ASC LIMIT 1",
        )
        .bind(provider.to_string())
        .bind(now)
        .fetch_optional(&self.db)
        .await?;
        let (key_id, key_value, status) =
            row.ok_or_else(|| anyhow!("no available key for {provider}"))?;
        if status != "active" {
            sqlx::query(
                "UPDATE keys SET status='active', cooldown_until=NULL, updated_at=?1 WHERE id=?2",
            )
            .bind(now)
            .bind(key_id)
            .execute(&self.db)
            .await?;
        }
        sqlx::query("UPDATE keys SET last_used_at=?1, updated_at=?1 WHERE id=?2")
            .bind(now)
            .bind(key_id)
            .execute(&self.db)
            .await?;
        Ok(Lease {
            key_id,
            key_value,
            lease_id: uuid::Uuid::new_v4().to_string(),
            provider,
        })
    }

    /// Record the outcome of a leased key use and update its cooldown state.
    pub async fn report(
        &self,
        key_id: i64,
        result: &str,
        retry_after: Option<i64>,
        rl_cooldown_secs: i64,
        quota_cooldown_secs: i64,
    ) -> Result<()> {
        let now = now_ts();
        let (status, cooldown): (&str, Option<i64>) = match result {
            "ok" => ("active", None),
            "429" => (
                "rate-limited",
                Some(now + retry_after.unwrap_or(rl_cooldown_secs)),
            ),
            "402" | "quota" => ("exhausted", Some(now + quota_cooldown_secs)),
            "auth-failed" => ("auth-failed", None),
            _ => ("active", None),
        };
        sqlx::query(
            "UPDATE keys SET status=?1, cooldown_until=?2, last_error=?3, updated_at=?4 WHERE id=?5",
        )
        .bind(status)
        .bind(cooldown)
        .bind(result)
        .bind(now)
        .bind(key_id)
        .execute(&self.db)
        .await?;
        Ok(())
    }

    pub async fn add_key(
        &self,
        provider: Provider,
        key: &str,
        account: Option<&str>,
    ) -> Result<i64> {
        let now = now_ts();
        let res = sqlx::query(
            "INSERT INTO keys (provider, account_team, key_value, status, created_at, updated_at) \
             VALUES (?1, ?2, ?3, 'active', ?4, ?4)",
        )
        .bind(provider.to_string())
        .bind(account)
        .bind(key)
        .bind(now)
        .execute(&self.db)
        .await;
        match res {
            Ok(r) => Ok(r.last_insert_rowid()),
            Err(e) => Err(anyhow!("add key failed (duplicate provider+key?): {e}")),
        }
    }

    pub async fn list_keys(&self) -> Result<Vec<KeyRow>> {
        let rows = sqlx::query_as::<_, KeyRow>(
            "SELECT id, provider, account_team, key_value, status, cooldown_until, \
             credits_remaining, last_used_at, last_error FROM keys ORDER BY provider, id",
        )
        .fetch_all(&self.db)
        .await?;
        Ok(rows)
    }

    pub async fn remove_key(&self, id: i64) -> Result<()> {
        let r = sqlx::query("DELETE FROM keys WHERE id=?1")
            .bind(id)
            .execute(&self.db)
            .await?;
        if r.rows_affected() == 0 {
            return Err(anyhow!("no key with id {id}"));
        }
        Ok(())
    }

    pub async fn create_user(&self, name: Option<&str>) -> Result<String> {
        let token = format!("sp-{}", uuid::Uuid::new_v4().simple());
        let now = now_ts();
        sqlx::query("INSERT INTO users (token, name, created_at) VALUES (?1, ?2, ?3)")
            .bind(&token)
            .bind(name)
            .bind(now)
            .execute(&self.db)
            .await?;
        Ok(token)
    }

    pub async fn list_users(&self) -> Result<Vec<UserRow>> {
        let rows = sqlx::query_as::<_, UserRow>(
            "SELECT id, token, name, created_at FROM users ORDER BY id",
        )
        .fetch_all(&self.db)
        .await?;
        Ok(rows)
    }

    pub async fn validate_user_token(&self, token: &str) -> Result<bool> {
        let r: Option<(i64,)> = sqlx::query_as("SELECT id FROM users WHERE token = ?1")
            .bind(token)
            .fetch_optional(&self.db)
            .await?;
        Ok(r.is_some())
    }

    pub async fn revoke_user(&self, token: &str) -> Result<()> {
        let r = sqlx::query("DELETE FROM users WHERE token = ?1")
            .bind(token)
            .execute(&self.db)
            .await?;
        if r.rows_affected() == 0 {
            return Err(anyhow!("no user with that token"));
        }
        Ok(())
    }

    pub async fn pool_stats(&self) -> Result<PoolStats> {
        let mut keys_by_status: HashMap<String, i64> = HashMap::new();
        let rows: Vec<(String, i64)> =
            sqlx::query_as("SELECT status, COUNT(*) FROM keys GROUP BY status")
                .fetch_all(&self.db)
                .await?;
        let mut total_keys = 0;
        for (status, n) in rows {
            total_keys += n;
            keys_by_status.insert(status, n);
        }
        let total_users: i64 = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM users")
            .fetch_one(&self.db)
            .await?
            .0;
        Ok(PoolStats {
            keys_by_status,
            total_keys,
            total_users,
        })
    }

    pub async fn create_admin_session(&self, ttl_secs: i64) -> Result<(String, i64)> {
        let token = uuid::Uuid::new_v4().simple().to_string();
        let now = now_ts();
        let exp = now + ttl_secs;
        sqlx::query(
            "INSERT INTO admin_sessions (token, created_at, expires_at) VALUES (?1, ?2, ?3)",
        )
        .bind(&token)
        .bind(now)
        .bind(exp)
        .execute(&self.db)
        .await?;
        Ok((token, exp))
    }

    pub async fn validate_admin_session(&self, token: &str) -> Result<bool> {
        let now = now_ts();
        let r: Option<(i64,)> =
            sqlx::query_as("SELECT 1 FROM admin_sessions WHERE token = ?1 AND expires_at > ?2")
                .bind(token)
                .bind(now)
                .fetch_optional(&self.db)
                .await?;
        Ok(r.is_some())
    }

    /// Background refresh: query each key's active usage endpoint and update
    /// `credits_remaining` / status. Keys whose provider has no usage endpoint
    /// (tavily) are skipped — they rely on passive 429 detection.
    pub async fn refresh_usage(
        &self,
        client: &reqwest::Client,
        rl_cooldown_secs: i64,
        quota_cooldown_secs: i64,
    ) -> Result<()> {
        let rows: Vec<(i64, String, String)> = sqlx::query_as(
            "SELECT id, key_value, provider FROM keys \
             WHERE status NOT IN ('auth-failed','disabled')",
        )
        .fetch_all(&self.db)
        .await?;
        for (id, key, provider_str) in rows {
            let Some(provider) = Provider::from_str(&provider_str) else {
                continue;
            };
            let Some(path) = provider.usage_path() else {
                continue;
            };
            let url = format!("{}{path}", provider.real_base_url());
            match client.get(&url).bearer_auth(&key).send().await {
                Ok(r) => {
                    let code = r.status().as_u16();
                    if code == 401 {
                        let _ = self.mark_status(id, "auth-failed", None, "usage:401").await;
                        continue;
                    }
                    if code == 429 {
                        let cd = Some(now_ts() + rl_cooldown_secs);
                        let _ = self.mark_status(id, "rate-limited", cd, "usage:429").await;
                        continue;
                    }
                    if !r.status().is_success() {
                        continue;
                    }
                    let body = r.text().await.unwrap_or_default();
                    if let Some(rem) = parse_remaining(&body, provider) {
                        let _ = self.set_credits(id, rem).await;
                        if rem == 0 {
                            let cd = Some(now_ts() + quota_cooldown_secs);
                            let _ = self
                                .mark_status(id, "exhausted", cd, "usage:0 credits")
                                .await;
                        }
                    }
                }
                Err(e) => tracing::warn!(key_id = id, error = %e, "usage query failed"),
            }
        }
        Ok(())
    }

    async fn mark_status(
        &self,
        id: i64,
        status: &str,
        cooldown: Option<i64>,
        error: &str,
    ) -> Result<()> {
        let now = now_ts();
        sqlx::query(
            "UPDATE keys SET status=?1, cooldown_until=?2, last_error=?3, updated_at=?4 WHERE id=?5",
        )
        .bind(status)
        .bind(cooldown)
        .bind(error)
        .bind(now)
        .bind(id)
        .execute(&self.db)
        .await?;
        Ok(())
    }

    async fn set_credits(&self, id: i64, credits: i64) -> Result<()> {
        let now = now_ts();
        sqlx::query("UPDATE keys SET credits_remaining=?1, updated_at=?2 WHERE id=?3")
            .bind(credits)
            .bind(now)
            .bind(id)
            .execute(&self.db)
            .await?;
        Ok(())
    }
}

pub fn now_ts() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

fn parse_remaining(body: &str, provider: Provider) -> Option<i64> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    match provider {
        Provider::Firecrawl => v.get("data")?.get("remaining_credits")?.as_i64(),
        Provider::Tavily => None,
    }
}
