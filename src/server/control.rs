//! Control plane (`/lease`, `/report`) + data plane (`/firecrawl/*`,
//! `/tavily/*`) + WebUI JSON API (`/api/v1/*`).

use crate::config::Config;
use crate::keypool::KeyPool;
use crate::provider::Provider;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{any, delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct AppState {
    pub pool: KeyPool,
    pub cfg: Config,
}

#[derive(Deserialize)]
pub struct LeaseQuery {
    pub provider: String,
}

#[derive(Serialize)]
pub struct LeaseResp {
    pub key: String,
    pub key_id: i64,
    pub lease_id: String,
    pub provider: String,
}

#[derive(Deserialize)]
pub struct ReportReq {
    pub key_id: i64,
    pub result: String,
    pub retry_after: Option<i64>,
}

pub fn control_router(state: AppState) -> Router {
    Router::new()
        // control plane (mode A)
        .route("/lease", get(lease))
        .route("/report", post(report))
        // data plane (mode B)
        .route(
            "/firecrawl/{*path}",
            any(crate::server::proxy::proxy_firecrawl),
        )
        .route("/tavily/{*path}", any(crate::server::proxy::proxy_tavily))
        // webui admin api
        .route("/api/v1/login", post(crate::server::webui_api::login))
        .route(
            "/api/v1/keys",
            get(crate::server::webui_api::list_keys).post(crate::server::webui_api::add_key),
        )
        .route(
            "/api/v1/keys/{id}",
            delete(crate::server::webui_api::remove_key),
        )
        .route(
            "/api/v1/users",
            get(crate::server::webui_api::list_users).post(crate::server::webui_api::create_user),
        )
        .route(
            "/api/v1/users/{token}",
            delete(crate::server::webui_api::revoke_user),
        )
        .route("/api/v1/status", get(crate::server::webui_api::status))
        // webui static assets (SPA embedded from webui/dist)
        .route("/ui", get(crate::server::webui_static::webui_root))
        .route("/ui/", get(crate::server::webui_static::webui_root))
        .route("/ui/{*path}", get(crate::server::webui_static::webui_path))
        .with_state(state)
}

async fn lease(
    State(st): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<LeaseQuery>,
) -> Result<Json<LeaseResp>, (StatusCode, String)> {
    check_control_token(&st, &headers)?;
    let provider = Provider::from_str(&q.provider)
        .ok_or((StatusCode::BAD_REQUEST, "invalid provider".into()))?;
    let lease = st
        .pool
        .lease(provider)
        .await
        .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, e.to_string()))?;
    Ok(Json(LeaseResp {
        key: lease.key_value,
        key_id: lease.key_id,
        lease_id: lease.lease_id,
        provider: lease.provider.to_string(),
    }))
}

async fn report(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ReportReq>,
) -> Result<StatusCode, (StatusCode, String)> {
    check_control_token(&st, &headers)?;
    st.pool
        .report(
            body.key_id,
            &body.result,
            body.retry_after,
            st.cfg.rotation.rate_limit_cooldown_secs,
            st.cfg.rotation.quota_cooldown_secs,
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

fn check_control_token(st: &AppState, headers: &HeaderMap) -> Result<(), (StatusCode, String)> {
    let expected = match &st.cfg.server.control_token {
        Some(t) => format!("Bearer {t}"),
        None => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "server has no control_token configured".into(),
            ))
        }
    };
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if auth != expected {
        return Err((StatusCode::UNAUTHORIZED, "invalid control token".into()));
    }
    Ok(())
}
