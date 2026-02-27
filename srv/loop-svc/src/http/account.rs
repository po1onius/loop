use std::sync::Arc;

use axum::{Json, extract::State};
use bcrypt::verify;
use http::StatusCode;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use loop_svc_model::user::User;
use serde::{Deserialize, Serialize};
use srv_common::{
    db_conn,
    http::{ExceptionHandle, HttpErr, InnerExceptionHandle},
    utils,
};
use time::OffsetDateTime;

use crate::{config::CONFIG, http::AppState};

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String, // user_id
    iss: String,
    exp: i64,
    iat: i64,
}

#[derive(Debug, Deserialize)]
struct LoginReq {
    email: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct TokenPairResp {
    access_token: String,
    expires_in: i64,
    refresh_token: String,
    refresh_expires_in: i64,
}

#[derive(Debug, Deserialize)]
struct RefreshReq {
    refresh_token: String,
}

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginReq>,
) -> Result<Json<TokenPairResp>, HttpErr> {
    let cur_user = User::select_by_account(&req.account, &mut db_conn!())
        .await
        .ieh()?
        .eh(StatusCode::BAD_REQUEST, "user not exist")?;

    verify(&req.password, &cur_user.pwd)
        .ieh()?
        .then_some(())
        .eh(StatusCode::BAD_REQUEST, "password error")?;

    let exp = utils::time_tool::cur_ts().ieh()? as usize + CONFIG.jwt_expire_duration;
    let claims = Claims {
        exp,
        user_id: cur_user.user_id,
    };

    let token = encode(
        &Header::new(Algorithm::RS256),
        &claims,
        &EncodingKey::from_rsa_pem(CONFIG.crypto.jwt_rsa_pri_key.as_bytes()).ieh()?,
    )
    .ieh()?;

    let (access_token, access_exp) = mint_access_token(&state, &user_id)?;
    let (refresh_token, refresh_exp) = mint_refresh_token(&state, &user_id).await?;

    Ok(Json(TokenPairResp {
        access_token,
        expires_in: access_exp,
        refresh_token,
        refresh_expires_in: refresh_exp,
    }))
}

pub async fn refresh(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RefreshReq>,
) -> Result<Json<TokenPairResp>, ApiError> {
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

    Ok(Json(TokenPairResp {
        access_token,
        expires_in: access_exp,
        refresh_token: new_refresh_token,
        refresh_expires_in: refresh_exp,
    }))
}

fn mint_access_token(state: &AppState, user_id: &str) -> Result<(String, i64), ApiError> {
    let now = OffsetDateTime::now_utc();
    let exp = now + state.access_ttl;

    let claims = Claims {
        sub: user_id.to_string(),
        iss: state.jwt_issuer.clone(),
        iat: now.unix_timestamp(),
        exp: exp.unix_timestamp(),
    };

    let token = jsonwebtoken::encode(&Header::default(), &claims, &state.jwt_enc)
        .map_err(|_| ApiError::Internal)?;

    Ok((token, state.access_ttl.whole_seconds()))
}

async fn mint_refresh_token(state: &AppState, user_id: &str) -> Result<(String, i64), ApiError> {
    let now = OffsetDateTime::now_utc();
    let exp = now + state.refresh_ttl;

    let raw = generate_refresh_token();
    let token_hash = hash_refresh_token(&raw);

    let id = Uuid::new_v4().to_string();

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
    // 32 bytes -> base64url 无 padding，长度约 43-44 字符
    let mut buf = [0u8; 32];
    fill_bytes(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

fn hash_refresh_token(token: &str) -> String {
    // 生产可以做：SHA256(pepper + token)，pepper 从 env 来
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
