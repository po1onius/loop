use crate::{
    config::CONFIG,
    http::{
        AppState, Claims, PatchPerm,
        err_key::{RTE, TMR, VCE, VCW, XE},
        util::{ckb_vc, generate_code, hex_encode},
    },
};
use axum::{Json, Router, extract::State, routing::post};
use base64::Engine;
use bcrypt::{DEFAULT_COST, hash, verify};
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
    account::{NewRefreshTokens, NewUser, RefreshTokens, User},
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

                let (access_token, access_exp) =
                    mint_access_token(&state, cur_user.user_id, cur_user.role)?;
                let (refresh_token, refresh_exp) =
                    mint_refresh_token(cur_user.user_id, conn).await?;
                Ok(Json(LoginResp {
                    access_token,
                    expires_in: access_exp,
                    refresh_token,
                    refresh_exp,
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
                let (refresh_token, user) =
                    RefreshTokens::select_join_user_by_token(&token_hash, conn)
                        .await
                        .ieh()?
                        .eh(StatusCode::BAD_REQUEST, "invalid refresh token")?;

                if refresh_token.expires_at <= now || refresh_token.revoked_at.is_some() {
                    return Err(HttpErr::ClientErr(
                        StatusCode::UNAUTHORIZED,
                        RTE.to_string(),
                    ));
                }

                RefreshTokens::expire(refresh_token.id, conn).await.ieh()?;

                let (access_token, access_exp) =
                    mint_access_token(&state, refresh_token.user_id, user.role)?;
                let (new_refresh_token, refresh_exp) =
                    mint_refresh_token(refresh_token.user_id, conn).await?;
                Ok(Json(LoginResp {
                    access_token,
                    expires_in: access_exp,
                    refresh_token: new_refresh_token,
                    refresh_exp,
                }))
            }
            .scope_boxed()
        })
        .await
}

fn mint_access_token(
    state: &AppState,
    user_id: i64,
    role: String,
) -> Result<(String, i64), HttpErr> {
    let exp = Utc::now().timestamp() + CONFIG.load().access_ttl;
    let claims = Claims {
        exp: exp as usize,
        user_id,

        perm_ver: CONFIG.load().perm.perm_ver,
        role: role,
        patch_perm: PatchPerm::default(),
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
    let vck = ckb_vc(&req.account);
    let vc = redis_conn!()
        .get::<_, Option<String>>(&vck)
        .await
        .ieh()?
        .eh(StatusCode::BAD_REQUEST, VCE)?;
    if vc != req.verify_code {
        return Err(HttpErr::ClientErr(StatusCode::BAD_REQUEST, VCW.to_string()));
    }

    if req.username.len() > 50 || req.account.len() > 100 || req.pwd.len() > 16 {
        return Err(HttpErr::ClientErr(StatusCode::BAD_REQUEST, XE.to_string()));
    }

    let hash_pwd = hash(&req.pwd, DEFAULT_COST).ieh()?;

    let new_user = NewUser {
        username: &req.username,
        account: &req.account,
        pwd: &hash_pwd,
    };

    User::insert(&new_user, &mut db_conn!()).await.ieh()?;
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
