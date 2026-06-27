-- search-proxy initial schema

CREATE TABLE IF NOT EXISTS keys (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    provider          TEXT    NOT NULL,              -- 'firecrawl' | 'tavily'
    account_team      TEXT,                           -- account/team tag (firecrawl limits are per-team)
    key_value         TEXT    NOT NULL,
    status            TEXT    NOT NULL DEFAULT 'active', -- active|rate-limited|exhausted|auth-failed|disabled
    cooldown_until    INTEGER,                        -- unix ts (seconds); NULL when not cooling down
    credits_remaining INTEGER,                        -- last known remaining credits (from active usage query)
    last_used_at      INTEGER,
    last_error        TEXT,
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL,
    UNIQUE(provider, key_value)
);

CREATE INDEX IF NOT EXISTS idx_keys_provider_status
    ON keys(provider, status);

CREATE TABLE IF NOT EXISTS users (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    token      TEXT    NOT NULL UNIQUE,               -- mode-B user token (sp-...)
    name       TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS usage_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER,
    provider    TEXT    NOT NULL,
    key_id      INTEGER,
    endpoint    TEXT,
    http_status INTEGER,
    bytes_in    INTEGER,
    bytes_out   INTEGER,
    ts          INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_usage_ts ON usage_log(ts);
CREATE INDEX IF NOT EXISTS idx_usage_provider_ts ON usage_log(provider, ts);

CREATE TABLE IF NOT EXISTS admin_sessions (
    token      TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);
