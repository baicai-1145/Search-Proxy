//! Data plane: HTTP reverse proxy for firecrawl (`/firecrawl/*`) and tavily
//! (`/tavily/*`). Validates a mode-B user token, replaces `Authorization` with
//! a leased pool key, forwards to the real API, streams the response back, and
//! on 429/402 transparently rotates the key and retries once.

use crate::provider::Provider;
use crate::server::control::AppState;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, Response, StatusCode};
use axum::response::IntoResponse;
use std::time::Duration;

const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailers",
    "transfer-encoding",
    "upgrade",
    "host",
    "content-length",
];
const MAX_BODY: usize = 16 * 1024 * 1024;
const MAX_RETRY: usize = 2;

pub async fn proxy_firecrawl(st: State<AppState>, req: Request<Body>) -> Response<Body> {
    proxy(req, st, Provider::Firecrawl).await
}

pub async fn proxy_tavily(st: State<AppState>, req: Request<Body>) -> Response<Body> {
    proxy(req, st, Provider::Tavily).await
}

async fn proxy(
    req: Request<Body>,
    State(st): State<AppState>,
    provider: Provider,
) -> Response<Body> {
    let token = match bearer(&req.headers()) {
        Some(t) => t,
        None => return (StatusCode::UNAUTHORIZED, "missing bearer token").into_response(),
    };
    match st.pool.validate_user_token(&token).await {
        Ok(true) => {}
        _ => return (StatusCode::UNAUTHORIZED, "invalid user token").into_response(),
    }

    let (parts, body) = req.into_parts();
    let suffix = parts
        .uri
        .path()
        .strip_prefix(provider.proxy_prefix())
        .unwrap_or("");
    let query = parts
        .uri
        .query()
        .map(|q| format!("?{q}"))
        .unwrap_or_default();
    let upstream = format!("{}{}{}", provider.real_base_url(), suffix, query);

    let body_bytes = match axum::body::to_bytes(body, MAX_BODY).await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "request body too large").into_response(),
    };

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("client build: {e}"),
            )
                .into_response()
        }
    };

    let mut last_limit: Option<reqwest::Response> = None;
    for attempt in 1..=MAX_RETRY {
        let lease = match st.pool.lease(provider).await {
            Ok(l) => l,
            Err(e) => {
                return (StatusCode::SERVICE_UNAVAILABLE, format!("no key: {e}")).into_response()
            }
        };
        let mut fwd = filter_headers(&parts.headers);
        match HeaderValue::from_str(&format!("Bearer {}", lease.key_value)) {
            Ok(v) => {
                fwd.insert("authorization", v);
            }
            Err(_) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, "bad key encoding").into_response()
            }
        }
        let resp = match client
            .request(parts.method.clone(), &upstream)
            .headers(fwd)
            .body(body_bytes.clone())
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(%provider, error = %e, "upstream send error");
                return (StatusCode::BAD_GATEWAY, format!("upstream error: {e}")).into_response();
            }
        };
        let code = resp.status().as_u16();
        if code == 429 || code == 402 {
            let result = if code == 402 { "402" } else { "429" };
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<i64>().ok());
            let _ = st
                .pool
                .report(
                    lease.key_id,
                    result,
                    retry_after,
                    st.cfg.rotation.rate_limit_cooldown_secs,
                    st.cfg.rotation.quota_cooldown_secs,
                )
                .await;
            tracing::warn!(%provider, key_id = lease.key_id, attempt, code, "upstream limit; rotating key");
            last_limit = Some(resp);
            continue;
        }
        let _ = st
            .pool
            .report(
                lease.key_id,
                "ok",
                None,
                st.cfg.rotation.rate_limit_cooldown_secs,
                st.cfg.rotation.quota_cooldown_secs,
            )
            .await;
        return forward_response(resp).await;
    }
    if let Some(r) = last_limit {
        return forward_response(r).await;
    }
    (StatusCode::SERVICE_UNAVAILABLE, "all keys exhausted").into_response()
}

async fn forward_response(resp: reqwest::Response) -> Response<Body> {
    let status = resp.status();
    let headers = filter_headers(resp.headers());
    let stream = resp.bytes_stream();
    let mut builder = Response::builder().status(status);
    for (k, v) in &headers {
        builder = builder.header(k.clone(), v.clone());
    }
    builder.body(Body::from_stream(stream)).unwrap_or_else(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "body build failed").into_response()
    })
}

fn bearer(headers: &HeaderMap) -> Option<String> {
    let v = headers.get("authorization")?.to_str().ok()?;
    let t = v.strip_prefix("Bearer ")?;
    Some(t.to_string())
}

fn filter_headers(src: &HeaderMap) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (name, value) in src.iter() {
        let lower = name.as_str().to_ascii_lowercase();
        if HOP_BY_HOP.iter().any(|h| *h == lower) {
            continue;
        }
        out.append(name.clone(), value.clone());
    }
    out
}
