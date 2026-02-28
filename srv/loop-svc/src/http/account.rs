use std::sync::Arc;

use crate::{
    config::CONFIG,
    http::{AppState, Claims},
};
use axum::{Json, extract::State};
use base64::Engine;
use bcrypt::verify;
use http::StatusCode;
use jsonwebtoken::{Algorithm, Header, encode};
use loop_dto::{LoginRequest, LoginResp};
use loop_svc_model::user::User;
use rand::{Rng, rngs::ThreadRng};
use sha2::{Digest, Sha256};
use srv_common::{
    db_conn,
    http::{ExceptionHandle, HttpErr, InnerExceptionHandle},
    utils,
};
use time::OffsetDateTime;
use uuid::Uuid;

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResp>, HttpErr> {
    let cur_user = User::select_by_account(&req.account, &mut db_conn!())
        .await
        .ieh()?
        .eh(StatusCode::BAD_REQUEST, "user not exist")?;

    verify(&req.password, &cur_user.pwd)
        .ieh()?
        .then_some(())
        .eh(StatusCode::BAD_REQUEST, "password error")?;

    let (access_token, access_exp) = mint_access_token(&state, cur_user.user_id)?;
    let (refresh_token, refresh_exp) = mint_refresh_token(&state, cur_user.user_id).await?;

    Ok(Json(LoginResp {
        access_token,
        expires_in: access_exp,
        refresh_token,
        refresh_expires_in: refresh_exp,
    }))
}

pub async fn refresh(
    State(state): State<Arc<AppState>>,
    Json(req): Json<LoginResp>,
) -> Result<Json<LoginResp>, HttpErr> {
    let now = OffsetDateTime::now_utc().unix_timestamp();

    // 1) 查 refresh token hash 是否存在且有效
    let token_hash = hash_refresh_token(&req.refresh_token);

    let row: Option<(String, String, i64, Option<i64>, Option<i64>)> = sqlx::query_as(
        r#"
        SELECT id, user_id, expires_at, revoked_at, used_at
        FROM refresh_tokens
        WHERE token_hash = ?
        "#,
    )
    .bind(&token_hash)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| ApiError::Internal)?;

    let (rt_id, user_id, expires_at, revoked_at, used_at) = row.ok_or(ApiError::Unauthorized)?;

    if expires_at <= now {
        return Err(ApiError::Unauthorized);
    }
    if revoked_at.is_some() || used_at.is_some() {
        // rotation 防重放：旧 token 不能再用
        return Err(ApiError::Unauthorized);
    }

    // 2) 标记旧 refresh token 已用（used_at）
    sqlx::query("UPDATE refresh_tokens SET used_at = ? WHERE id = ?")
        .bind(now)
        .bind(&rt_id)
        .execute(&state.db)
        .await
        .map_err(|_| ApiError::Internal)?;

    // 3) 颁发新的 access + refresh（refresh rotation）
    let (access_token, access_exp) = mint_access_token(&state, &user_id)?;
    let (new_refresh_token, refresh_exp) = mint_refresh_token(&state, &user_id).await?;

    Ok(Json(LoginResp {
        access_token,
        expires_in: access_exp,
        refresh_token: new_refresh_token,
        refresh_expires_in: refresh_exp,
    }))
}

fn mint_access_token(state: &AppState, user_id: Uuid) -> Result<(String, usize), HttpErr> {
    let exp = utils::time_tool::cur_ts().ieh()? as usize + CONFIG.access_ttl;
    let claims = Claims {
        exp,
        user_id,
        perm_ver: 1,
        perms: vec![1],
    };

    let token = encode(&Header::new(Algorithm::RS256), &claims, &state.jwt_enc).ieh()?;

    Ok((token, CONFIG.access_ttl))
}

async fn mint_refresh_token(state: &AppState, user_id: Uuid) -> Result<(String, usize), HttpErr> {
    let exp = utils::time_tool::cur_ts().ieh()? as usize + CONFIG.refresh_ttl;

    let raw = generate_refresh_token();
    let token_hash = hash_refresh_token(&raw);

    let id = Uuid::now_v7().to_string();

    sqlx::query(
        r#"
        INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at, created_at)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(user_id)
    .bind(&token_hash)
    .bind(exp.unix_timestamp())
    .bind(now.unix_timestamp())
    .execute(&state.db)
    .await
    .map_err(|e| {
        warn!("insert refresh token failed: {e}");
        ApiError::Internal
    })?;

    Ok((raw, state.refresh_ttl.whole_seconds()))
}

fn generate_refresh_token() -> String {
    let mut buf = [0u8; 32];
    let mut rng = ThreadRng::default();
    rng.fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

fn hash_refresh_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let out = hasher.finalize();
    hex::encode(out)
}

// 用于 hex::encode
mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }
}
