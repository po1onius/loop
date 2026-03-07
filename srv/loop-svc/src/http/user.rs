use crate::{
    config::CONFIG,
    http::{
        AppState, Claims,
        err_key::{RTE, TMR},
        util::{ckb_vc, generate_code, hex_encode},
    },
};
use axum::{Json, Router, extract::State, routing::post};
use base64::Engine;
use bcrypt::verify;
use chrono::{Duration, Utc};
use diesel_async::{AsyncConnection, scoped_futures::ScopedFutureExt};
use http::StatusCode;
use jsonwebtoken::{Algorithm, Header, encode};
use loop_dto::{
    LoginRequest, LoginResp, RefreshTokenRequest, RegisterRequest, VerifyCodeRequest,
    VerifyCodeResp,
};
use loop_svc_model::{
    DieselConn,
    account::{NewRefreshTokens, RefreshTokens, User},
};
use rand::{Rng, rngs::ThreadRng};
use redis::AsyncCommands;
use sha2::{Digest, Sha256};
use srv_common::{
    db_conn,
    http::{ExceptionHandle, HttpErr, InnerExceptionHandle},
    redis_conn,
};

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResp>, HttpErr> {
    db_conn!()
        .transaction(|conn| {
            async {
                let cur_user = User::select_by_account(&req.account, conn)
                    .await
                    .ieh()?
                    .eh(StatusCode::BAD_REQUEST, "user not exist")?;

                verify(&req.password, &cur_user.pwd)
                    .ieh()?
                    .then_some(())
                    .eh(StatusCode::BAD_REQUEST, "password error")?;

                let (access_token, access_exp) = mint_access_token(&state, &cur_user)?;
                let (refresh_token, refresh_exp) =
                    mint_refresh_token(cur_user.user_id, conn).await?;
                Ok(Json(LoginResp {
                    access_token,
                    expires_in: access_exp,
                    refresh_token,
                }))
            }
            .scope_boxed()
        })
        .await
}

pub async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshTokenRequest>,
) -> Result<Json<LoginResp>, HttpErr> {
    let now = Utc::now();
    let token_hash = hash_refresh_token(&req.refresh_token);
    db_conn!()
        .transaction(|conn| {
            async {
                let row = RefreshTokens::select_by_token_hash(&token_hash, conn)
                    .await
                    .ieh()?
                    .eh(StatusCode::BAD_REQUEST, "invalid refresh token")?;

                if row.expires_at <= now || row.revoked_at.is_some() {
                    return Err(HttpErr::ClientErr(
                        StatusCode::UNAUTHORIZED,
                        RTE.to_string(),
                    ));
                }

                RefreshTokens::expire(row.id, conn).await.ieh()?;

                let (access_token, access_exp) = mint_access_token(&state, &row)?;
                let (new_refresh_token, refresh_exp) =
                    mint_refresh_token(row.user_id, conn).await?;
                Ok(Json(LoginResp {
                    access_token,
                    expires_in: access_exp,
                    refresh_token: new_refresh_token,
                }))
            }
            .scope_boxed()
        })
        .await
}

fn mint_access_token(state: &AppState, user: &User) -> Result<(String, i64), HttpErr> {
    let exp = Utc::now().timestamp() + CONFIG.load().access_ttl;
    let claims = Claims {
        exp: exp as usize,
        user_id: user.user_id,

        perm_ver: CONFIG.load().perm.perm_ver,
        role: user.role,
        ..Default
    };

    let token = encode(&Header::new(Algorithm::RS256), &claims, &state.jwt_enc).ieh()?;

    Ok((token, CONFIG.load().access_ttl))
}

async fn mint_refresh_token(user_id: i64, conn: &mut DieselConn) -> Result<(String, i64), HttpErr> {
    let ttl = Duration::seconds(CONFIG.load().refresh_ttl);
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

    RefreshTokens::insert(new, conn).await.ieh()?;

    Ok((raw, CONFIG.load().refresh_ttl))
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
    hex_encode(out)
}

pub async fn register(
    //State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<(), HttpErr> {
    Ok(())
}

pub async fn verify_code(
    Json(req): Json<VerifyCodeRequest>,
) -> Result<Json<VerifyCodeResp>, HttpErr> {
    let vck = ckb_vc(req.account);
    let vc: Option<String> = redis_conn!().get(&vck).await.ieh()?;
    if vc.is_some() {
        return Err(HttpErr::ClientErr(StatusCode::BAD_REQUEST, TMR.to_string()));
    }
    let code = generate_code();
    redis_conn!().set::<_, _, ()>(&vck, &code).await.ieh()?;
    Ok(Json(VerifyCodeResp { code }))
}

pub fn route(state: AppState) -> Router {
    Router::new()
        .nest(
            "/user",
            Router::new()
                .route("/login", post(login))
                .route("/refresh_token", post(refresh))
                .route("/register", post(register))
                .route("/verify_code", post(verify_code)),
        )
        .with_state(state)
}
