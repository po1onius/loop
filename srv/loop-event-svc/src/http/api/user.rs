use crate::{
    config::CONFIG,
    db_conn,
    http::{
        AppState, Claims, HttpErr, OptionExt, PatchPerm, ResultExt,
        err_key::*,
        util::{ckb_vc, generate_code, hex_encode},
    },
    redis_conn,
    service::notify::email_code,
};
use axum::{Json, Router, extract::State, routing::post};
use base64::Engine;
use bcrypt::{DEFAULT_COST, hash, verify};
use chrono::{Duration, Utc};
use diesel::result::{DatabaseErrorKind, Error as DieselError};
use diesel_async::AsyncConnection;
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

#[tracing::instrument(name = "user.login", skip_all)]
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResp>, HttpErr> {
    db_conn!()
        .transaction::<_, HttpErr, _>(async |conn| {
            let cur_user = User::select_by_account(&req.account, conn)
                .await
                .internal(DB_ERROR)?
                .client(StatusCode::BAD_REQUEST, USER_NOT_EXIST)?;

            verify(&req.password, &cur_user.pwd)
                .internal(PASSWORD_VERIFY_ERROR)?
                .then_some(())
                .client(StatusCode::BAD_REQUEST, PASSWORD_ERROR)?;

            let (access_token, access_exp) =
                mint_access_token(&state, cur_user.user_id, cur_user.role)?;
            let (refresh_token, refresh_exp) = mint_refresh_token(cur_user.user_id, conn).await?;
            Ok(Json(LoginResp {
                access_token,
                expires_in: access_exp,
                refresh_token,
                refresh_exp,
            }))
        })
        .await
}

#[tracing::instrument(name = "user.refresh", skip_all)]
pub async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshTokenRequest>,
) -> Result<Json<LoginResp>, HttpErr> {
    let now = Utc::now();
    let token_hash = hash_refresh_token(&req.refresh_token);
    db_conn!()
        .transaction::<_, HttpErr, _>(async |conn| {
            let (refresh_token, user) = RefreshTokens::select_join_user_by_token(&token_hash, conn)
                .await
                .internal(DB_ERROR)?
                .client(StatusCode::BAD_REQUEST, INVALID_REFRESH_TOKEN)?;

            if refresh_token.expires_at <= now || refresh_token.revoked_at.is_some() {
                return Err(HttpErr::client(
                    StatusCode::UNAUTHORIZED,
                    REFRESH_TOKEN_EXPIRED,
                ));
            }

            RefreshTokens::expire(refresh_token.id, conn)
                .await
                .internal(DB_ERROR)?;

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

    let token = encode(&Header::new(Algorithm::RS256), &claims, &state.jwt_enc)
        .internal(JWT_ENCODE_ERROR)?;

    Ok((token, CONFIG.load().access_ttl))
}

#[tracing::instrument(name = "user.refresh_token.mint", skip_all)]
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

    RefreshTokens::insert(new, conn).await.internal(DB_ERROR)?;

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

#[tracing::instrument(name = "user.register", skip_all)]
pub async fn register(Json(req): Json<RegisterRequest>) -> Result<(), HttpErr> {
    let vck = ckb_vc(&req.account);
    let vc = redis_conn!()
        .get::<_, Option<String>>(&vck)
        .await
        .internal(REDIS_ERROR)?
        .client(StatusCode::BAD_REQUEST, VERIFY_CODE_EXPIRED)?;
    if vc != req.verify_code {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, VERIFY_CODE_WRONG));
    }

    if req.username.len() > 50 || req.account.len() > 100 || req.pwd.len() > 16 {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }

    let hash_pwd = hash(&req.pwd, DEFAULT_COST).internal(PASSWORD_HASH_ERROR)?;

    let new_user = NewUser {
        username: &req.username,
        account: &req.account,
        pwd: &hash_pwd,
    };

    User::insert(&new_user, &mut db_conn!())
        .await
        .map_err(map_user_insert_error)?;
    Ok(())
}

#[tracing::instrument(name = "user.verify_code", skip_all)]
pub async fn verify_code(
    Json(req): Json<VerifyCodeRequest>,
) -> Result<Json<VerifyCodeResp>, HttpErr> {
    let user = User::select_by_account(&req.account, &mut db_conn!())
        .await
        .internal(DB_ERROR)?;
    if user.is_some() {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, ALREADY_EXIST));
    }
    let vck = ckb_vc(&req.account);
    let vc: Option<String> = redis_conn!().get(&vck).await.internal(REDIS_ERROR)?;
    if vc.is_some() {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, TOO_MANY_REQUESTS));
    }
    let code = generate_code();
    email_code(&code, &req.account, "", &None)
        .await
        .internal(EMAIL_SEND_ERROR)?;
    redis_conn!()
        .set::<_, _, ()>(&vck, &code)
        .await
        .internal(REDIS_ERROR)?;
    Ok(Json(VerifyCodeResp { code }))
}

fn map_user_insert_error(err: DieselError) -> HttpErr {
    match err {
        DieselError::DatabaseError(DatabaseErrorKind::UniqueViolation, _) => {
            HttpErr::client(StatusCode::CONFLICT, ALREADY_EXIST)
        }
        err => HttpErr::internal(DB_ERROR, err),
    }
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
