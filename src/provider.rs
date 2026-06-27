use serde::{Deserialize, Serialize};
use std::fmt;

/// A supported search provider whose CLI we proxy for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Firecrawl,
    Tavily,
}

impl Provider {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "firecrawl" => Some(Provider::Firecrawl),
            "tavily" | "tvly" => Some(Provider::Tavily),
            _ => None,
        }
    }

    /// Real upstream API base URL (where mode-A CLI connects directly, and mode-B
    /// proxy forwards to).
    pub fn real_base_url(&self) -> &'static str {
        match self {
            Provider::Firecrawl => "https://api.firecrawl.dev",
            Provider::Tavily => "https://api.tavily.com",
        }
    }

    /// Path prefix the mode-B reverse proxy mounts for this provider.
    pub fn proxy_prefix(&self) -> &'static str {
        match self {
            Provider::Firecrawl => "/firecrawl",
            Provider::Tavily => "/tavily",
        }
    }

    /// Env var the real CLI reads for the API key.
    pub fn api_key_env(&self) -> &'static str {
        match self {
            Provider::Firecrawl => "FIRECRAWL_API_KEY",
            Provider::Tavily => "TAVILY_API_KEY",
        }
    }

    /// Env var the real CLI reads for the base URL override.
    pub fn base_url_env(&self) -> &'static str {
        match self {
            Provider::Firecrawl => "FIRECRAWL_API_URL",
            Provider::Tavily => "TAVILY_API_BASE_URL",
        }
    }

    /// Real CLI binary name (used by `wrap` and `install` shims).
    pub fn cli_bin(&self) -> &'static str {
        match self {
            Provider::Firecrawl => "firecrawl",
            Provider::Tavily => "tvly",
        }
    }

    /// Optional active usage-query endpoint. `None` means rely on passive
    /// 429/402 detection only (tavily is rate-limit based, no credit endpoint).
    pub fn usage_path(&self) -> Option<&'static str> {
        match self {
            Provider::Firecrawl => Some("/v1/team/credit-usage"),
            Provider::Tavily => None,
        }
    }
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Provider::Firecrawl => write!(f, "firecrawl"),
            Provider::Tavily => write!(f, "tavily"),
        }
    }
}
