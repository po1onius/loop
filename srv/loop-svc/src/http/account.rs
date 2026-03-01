use crate::{
    config::CONFIG,
    http::{AppState, Claims},
};
use axum::{Json, extract::State};
use base64::Engine;
use bcrypt::verify;
use chrono::{Duration, Utc};
use http::StatusCode;
use jsonwebtoken::{Algorithm, Header, encode};
use loop_dto::{LoginRequest, LoginResp};
use loop_svc_model::account::{NewRefreshTokens, RefreshTokens, User};
use rand::{Rng, rngs::ThreadRng};
use sha2::{Digest, Sha256};
use srv_common::{
    db_conn,
    http::{ExceptionHandle, HttpErr, InnerExceptionHandle},
};
use std::sync::Arc;

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
    State(state): State<AppState>,
    Json(req): Json<LoginResp>,
) -> Result<Json<LoginResp>, HttpErr> {
    let now = Utc::now();
    let token_hash = hash_refresh_token(&req.refresh_token);

    let row = RefreshTokens::select_by_token_hash(&token_hash, &mut db_conn!())
        .await
        .ieh()?
        .eh(StatusCode::BAD_REQUEST, "")?;

    if row.expires_at <= now || row.revoked_at.is_some() {
        return Err(HttpErr::ClientErr(
            StatusCode::UNAUTHORIZED,
            "refresh token expction".to_string(),
        ));
    }

    RefreshTokens::expire(row.id, &mut db_conn!()).await.ieh()?;

    // 3) 颁发新的 access + refresh（refresh rotation）
    let (access_token, access_exp) = mint_access_token(&state, row.user_id)?;
    let (new_refresh_token, refresh_exp) = mint_refresh_token(&state, row.user_id).await?;

    Ok(Json(LoginResp {
        access_token,
        expires_in: access_exp,
        refresh_token: new_refresh_token,
        refresh_expires_in: refresh_exp,
    }))
}

fn mint_access_token(state: &AppState, user_id: i64) -> Result<(String, i64), HttpErr> {
    let exp = Utc::now().timestamp() + CONFIG.access_ttl;
    let claims = Claims {
        exp: exp as usize,
        user_id,
        // TODO
        perm_ver: 1,
        perms: vec![1],
    };

    let token = encode(&Header::new(Algorithm::RS256), &claims, &state.jwt_enc).ieh()?;

    Ok((token, CONFIG.access_ttl))
}

async fn mint_refresh_token(state: &AppState, user_id: i64) -> Result<(String, i64), HttpErr> {
    let ttl = Duration::seconds(CONFIG.refresh_ttl);
    let expires_at = Utc::now() + ttl;

    let raw = generate_refresh_token();
    let token_hash = hash_refresh_token(&raw);

    let new = NewRefreshTokens {
        user_id,
        token_hash: &token_hash,
        device_id: None,
        expires_at,
        revoked_at: None,
        ip_address: None,
        user_agent: None,
    };

    Ok((raw, CONFIG.refresh_ttl))
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
