//! WebUI JSON API under `/api/v1/*`, authenticated with admin sessions issued
//! by `POST /api/v1/login`. Static WebUI assets at `/ui` are added in M5.

use crate::keypool::KeyRow;
use crate::provider::Provider;
use crate::server::control::AppState;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct LoginReq {
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResp {
    pub token: String,
    pub expires_at: i64,
}

#[derive(Deserialize)]
pub struct KeyAddReq {
    pub provider: String,
    pub key: String,
    pub account: Option<String>,
}

#[derive(Serialize)]
pub struct KeyApi {
    pub id: i64,
    pub provider: String,
    pub account_team: Option<String>,
    pub status: String,
    pub cooldown_until: Option<i64>,
    pub credits_remaining: Option<i64>,
    pub last_used_at: Option<i64>,
    pub last_error: Option<String>,
    pub key_masked: String,
}

#[derive(Deserialize)]
pub struct UserCreateReq {
    pub name: Option<String>,
}

#[derive(Serialize)]
pub struct UserCreateResp {
    pub token: String,
}

pub async fn login(
    State(st): State<AppState>,
    Json(body): Json<LoginReq>,
) -> Result<Json<LoginResp>, (StatusCode, String)> {
    let password = st.cfg.admin.password.as_deref().ok_or((
        StatusCode::FORBIDDEN,
        "admin login disabled (no admin.password set)".into(),
    ))?;
    if password != body.password {
        return Err((StatusCode::UNAUTHORIZED, "wrong password".into()));
    }
    let (token, exp) = st
        .pool
        .create_admin_session(st.cfg.admin.session_ttl_secs)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(LoginResp {
        token,
        expires_at: exp,
    }))
}

pub async fn list_keys(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<KeyApi>>, (StatusCode, String)> {
    check_admin_session(&st, &headers).await?;
    let rows = st
        .pool
        .list_keys()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(rows.into_iter().map(key_row_to_api).collect()))
}

pub async fn add_key(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<KeyAddReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    check_admin_session(&st, &headers).await?;
    let provider = Provider::from_str(&body.provider)
        .ok_or((StatusCode::BAD_REQUEST, "invalid provider".into()))?;
    let id = st
        .pool
        .add_key(provider, &body.key, body.account.as_deref())
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(serde_json::json!({"id": id})))
}

pub async fn remove_key(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, String)> {
    check_admin_session(&st, &headers).await?;
    st.pool
        .remove_key(id)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_users(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::keypool::UserRow>>, (StatusCode, String)> {
    check_admin_session(&st, &headers).await?;
    let users = st
        .pool
        .list_users()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(users))
}

pub async fn create_user(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<UserCreateReq>,
) -> Result<Json<UserCreateResp>, (StatusCode, String)> {
    check_admin_session(&st, &headers).await?;
    let token = st
        .pool
        .create_user(body.name.as_deref())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(UserCreateResp { token }))
}

pub async fn revoke_user(
    State(st): State<AppState>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    check_admin_session(&st, &headers).await?;
    st.pool
        .revoke_user(&token)
        .await
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn status(
    State(st): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<crate::keypool::PoolStats>, (StatusCode, String)> {
    check_admin_session(&st, &headers).await?;
    let stats = st
        .pool
        .pool_stats()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(stats))
}

async fn check_admin_session(
    st: &AppState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, String)> {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let token = auth
        .strip_prefix("Bearer ")
        .ok_or((StatusCode::UNAUTHORIZED, "missing bearer session".into()))?;
    match st.pool.validate_admin_session(token).await {
        Ok(true) => Ok(()),
        _ => Err((
            StatusCode::UNAUTHORIZED,
            "invalid or expired session".into(),
        )),
    }
}

fn key_row_to_api(r: KeyRow) -> KeyApi {
    KeyApi {
        id: r.id,
        provider: r.provider,
        account_team: r.account_team,
        status: r.status,
        cooldown_until: r.cooldown_until,
        credits_remaining: r.credits_remaining,
        last_used_at: r.last_used_at,
        last_error: r.last_error,
        key_masked: mask_key(&r.key_value),
    }
}

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
