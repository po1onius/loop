use crate::{
    config::config,
    db_conn,
    http::{
        AppRoutes, AppState, Claims, HttpErr, OptionExt, ResultExt,
        err_key::*,
        util::{ckb_vc, generate_code, hex_encode},
    },
    redis_conn,
    service::notify::email_code,
};
use axum::{Json, extract::State, routing::post};
use base64::Engine;
use bcrypt::{DEFAULT_COST, hash, verify};
use chrono::{Duration, Utc};
use diesel::result::{DatabaseErrorKind, Error as DieselError};
use diesel_async::AsyncConnection;
use http::{Method, StatusCode};
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

const DEFAULT_USER_ROLE: &str = "user";
const VERIFY_CODE_TTL_SECONDS: u64 = 120;
const VERIFY_CODE_EXPIRE_MINUTES: u64 = VERIFY_CODE_TTL_SECONDS / 60;

#[tracing::instrument(
    name = "user.login",
    skip_all,
    fields(user.id = tracing::field::Empty, auth.role = tracing::field::Empty)
)]
pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResp>, HttpErr> {
    let account = normalize_account(&req.account);
    let cur_user = User::select_by_account(&account, &mut db_conn!())
        .await
        .internal(DB_ERROR)?
        .client(StatusCode::BAD_REQUEST, USER_NOT_EXIST)?;
    let span = tracing::Span::current();
    span.record("user.id", cur_user.user_id);
    span.record("auth.role", tracing::field::display(&cur_user.role));

    verify_password(req.password, cur_user.pwd.clone())
        .await?
        .then_some(())
        .client(StatusCode::BAD_REQUEST, PASSWORD_ERROR)?;

    let (access_token, access_exp) = mint_access_token(&state, cur_user.user_id, cur_user.role)?;
    let (refresh_token, refresh_exp) =
        mint_refresh_token(cur_user.user_id, &mut db_conn!()).await?;
    Ok(Json(LoginResp {
        access_token,
        expires_in: access_exp,
        refresh_token,
        refresh_exp,
    }))
}

#[tracing::instrument(
    name = "user.refresh",
    skip_all,
    fields(user.id = tracing::field::Empty, auth.role = tracing::field::Empty)
)]
pub async fn refresh(
    State(state): State<AppState>,
    Json(req): Json<RefreshTokenRequest>,
) -> Result<Json<LoginResp>, HttpErr> {
    let now = Utc::now();
    let token_hash = hash_refresh_token(&req.refresh_token);
    db_conn!()
        .transaction::<_, HttpErr, _>(async |conn| {
            let refresh_token = RefreshTokens::consume_by_token_hash(&token_hash, conn)
                .await
                .internal(DB_ERROR)?
                .client(StatusCode::BAD_REQUEST, INVALID_REFRESH_TOKEN)?;

            if refresh_token.expires_at <= now {
                return Err(HttpErr::client(
                    StatusCode::UNAUTHORIZED,
                    REFRESH_TOKEN_EXPIRED,
                ));
            }
            tracing::Span::current().record("user.id", refresh_token.user_id);

            let user = User::select_by_user_id(refresh_token.user_id, conn)
                .await
                .internal(DB_ERROR)?
                .client(StatusCode::BAD_REQUEST, USER_NOT_EXIST)?;
            tracing::Span::current().record("auth.role", tracing::field::display(&user.role));

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

#[tracing::instrument(
    name = "user.access_token.mint",
    skip_all,
    fields(user.id = user_id, auth.role = %role)
)]
fn mint_access_token(
    state: &AppState,
    user_id: i64,
    role: String,
) -> Result<(String, i64), HttpErr> {
    let cfg = config();
    let exp = Utc::now().timestamp() + cfg.access_ttl;
    let claims = Claims {
        exp: exp as usize,
        user_id,

        perm_ver: cfg.perm.perm_ver,
        role,
    };

    let token = encode(&Header::new(Algorithm::RS256), &claims, &state.jwt_enc)
        .internal(JWT_ENCODE_ERROR)?;

    Ok((token, cfg.access_ttl))
}

#[tracing::instrument(
    name = "user.refresh_token.mint",
    skip_all,
    fields(user.id = user_id)
)]
async fn mint_refresh_token(user_id: i64, conn: &mut DieselConn) -> Result<(String, i64), HttpErr> {
    let refresh_ttl = config().refresh_ttl;
    let ttl = Duration::seconds(refresh_ttl);
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

    Ok((raw, refresh_ttl))
}

#[tracing::instrument(name = "user.password.verify", skip_all)]
async fn verify_password(password: String, hash_pwd: String) -> Result<bool, HttpErr> {
    tokio::task::spawn_blocking(move || verify(password, &hash_pwd))
        .await
        .internal(PASSWORD_VERIFY_ERROR)?
        .internal(PASSWORD_VERIFY_ERROR)
}

#[tracing::instrument(name = "user.password.hash", skip_all)]
async fn hash_password(password: String) -> Result<String, HttpErr> {
    tokio::task::spawn_blocking(move || hash(password, DEFAULT_COST))
        .await
        .internal(PASSWORD_HASH_ERROR)?
        .internal(PASSWORD_HASH_ERROR)
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
    let account = normalize_account(&req.account);
    let username = req.username.trim().to_string();
    let verify_code = req.verify_code.trim().to_string();
    let vck = ckb_vc(&account);
    let mut redis = redis_conn!();
    let vc = redis
        .get::<_, Option<String>>(&vck)
        .await
        .internal(REDIS_ERROR)?
        .client(StatusCode::BAD_REQUEST, VERIFY_CODE_EXPIRED)?;
    if vc != verify_code {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, VERIFY_CODE_WRONG));
    }

    if username.is_empty() || username.len() > 50 || account.len() > 100 || req.pwd.len() > 72 {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }

    let hash_pwd = hash_password(req.pwd).await?;

    let new_user = NewUser {
        username: &username,
        account: &account,
        pwd: &hash_pwd,
        role: DEFAULT_USER_ROLE,
    };

    User::insert(&new_user, &mut db_conn!())
        .await
        .map_err(map_user_insert_error)?;
    if let Err(err) = redis.del::<_, ()>(&vck).await {
        tracing::warn!(
            event = "verify_code.cleanup_failed",
            error_kind = "redis",
            error_source = ?err,
            "failed to delete used verify code"
        );
    }
    Ok(())
}

#[tracing::instrument(name = "user.verify_code", skip_all)]
pub async fn verify_code(
    Json(req): Json<VerifyCodeRequest>,
) -> Result<Json<VerifyCodeResp>, HttpErr> {
    let account = normalize_account(&req.account);
    let user = User::select_by_account(&account, &mut db_conn!())
        .await
        .internal(DB_ERROR)?;
    if user.is_some() {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, ALREADY_EXIST));
    }
    let vck = ckb_vc(&account);
    let mut redis = redis_conn!();
    let code = generate_code();
    let stored = redis::cmd("SET")
        .arg(&vck)
        .arg(&code)
        .arg("NX")
        .arg("EX")
        .arg(VERIFY_CODE_TTL_SECONDS)
        .query_async::<Option<String>>(&mut redis)
        .await
        .internal(REDIS_ERROR)?;
    if stored.is_none() {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, TOO_MANY_REQUESTS));
    }
    if let Err(err) = email_code(
        &code,
        &account,
        "Loop 验证码",
        &None,
        VERIFY_CODE_EXPIRE_MINUTES,
    )
    .await
    {
        if let Err(del_err) = redis.del::<_, ()>(&vck).await {
            tracing::warn!(
                event = "verify_code.cleanup_failed",
                error_kind = "redis",
                error_source = ?del_err,
                "failed to delete verify code after email failure"
            );
        }
        return Err(HttpErr::internal(EMAIL_SEND_ERROR, err));
    }
    Ok(Json(VerifyCodeResp {}))
}

fn normalize_account(account: &str) -> String {
    account.trim().to_ascii_lowercase()
}

fn map_user_insert_error(err: DieselError) -> HttpErr {
    match err {
        DieselError::DatabaseError(DatabaseErrorKind::UniqueViolation, _) => {
            HttpErr::client(StatusCode::CONFLICT, ALREADY_EXIST)
        }
        err => HttpErr::internal(DB_ERROR, err),
    }
}

pub fn route(state: AppState) -> AppRoutes {
    AppRoutes::<AppState>::new()
        .public(Method::POST, "/user/login", post(login))
        .public(Method::POST, "/user/refresh_token", post(refresh))
        .public(Method::POST, "/user/register", post(register))
        .public(Method::POST, "/user/verify_code", post(verify_code))
        .with_state(state)
}
