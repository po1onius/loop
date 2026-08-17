use crate::{
    config::config,
    db_conn,
    http::{
        AppRoutes, AppState, AuthInfo, Claims, HttpErr, OptionExt, ResultExt,
        err_key::*,
        util::{ckb_vc, generate_code, hex_encode},
    },
    redis_conn,
    service::notify::email_code,
};
use axum::{
    Json,
    extract::State,
    routing::{get, patch, post},
};
use base64::Engine;
use bcrypt::{DEFAULT_COST, hash, verify};
use chrono::{Duration, Utc};
use diesel::result::{DatabaseErrorKind, Error as DieselError};
use diesel_async::AsyncConnection;
use http::{Method, StatusCode};
use jsonwebtoken::{Algorithm, Header, encode};
use loop_dto::{
    CurrentUserResp, LoginRequest, LoginResp, RefreshTokenRequest, RegisterRequest,
    UpdateUserAvatarRequest, VerifyCodeRequest, VerifyCodeResp,
};
use loop_svc_model::{
    DieselConn,
    account::{NewRefreshTokens, NewUser, RefreshTokens, User},
    event::MediaAsset,
};
use rand::{Rng, rngs::ThreadRng};
use redis::AsyncCommands;
use sha2::{Digest, Sha256};
use uuid::Uuid;

const DEFAULT_USER_ROLE: &str = "user";
const USER_PROFILE_READ_PERMISSION: &str = "user.profile.read";
const USER_PROFILE_UPDATE_PERMISSION: &str = "user.profile.update";
const VERIFY_CODE_TTL_SECONDS: u64 = 120;
const VERIFY_CODE_EXPIRE_MINUTES: u64 = VERIFY_CODE_TTL_SECONDS / 60;

enum RefreshAttempt {
    Rotated(LoginResp),
    Expired,
    Invalid,
    Reused {
        family_id: Uuid,
        user_id: i64,
        revoked_count: usize,
    },
}

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
        mint_refresh_token(cur_user.user_id, None, &mut db_conn!()).await?;
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
    let attempt = db_conn!()
        .transaction::<RefreshAttempt, HttpErr, _>(async |conn| {
            // 先定位并锁定 token family，再消费当前 token。相同 family 的刷新、
            // 重放检测和注销因此严格串行，不会在轮换与撤销之间留下竞态窗口。
            let known_token = RefreshTokens::select_by_token_hash(&token_hash, conn)
                .await
                .internal(DB_ERROR)?;
            let Some(known_token) = known_token else {
                return Ok(RefreshAttempt::Invalid);
            };
            RefreshTokens::lock_family(known_token.family_id, conn)
                .await
                .internal(DB_ERROR)?;

            let Some(refresh_token) = RefreshTokens::consume_by_token_hash(&token_hash, conn)
                .await
                .internal(DB_ERROR)?
            else {
                // 哈希存在但已经被消费，说明同一 refresh token 被重复使用。此时撤销
                // 整个 token family 的活跃成员，阻断窃取者与合法客户端之间的刷新竞态。
                let revoked_count =
                    RefreshTokens::revoke_active_family(known_token.family_id, conn)
                        .await
                        .internal(DB_ERROR)?;
                return Ok(RefreshAttempt::Reused {
                    family_id: known_token.family_id,
                    user_id: known_token.user_id,
                    revoked_count,
                });
            };

            if refresh_token.expires_at <= now {
                return Ok(RefreshAttempt::Expired);
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
                mint_refresh_token(refresh_token.user_id, Some(refresh_token.family_id), conn)
                    .await?;
            Ok(RefreshAttempt::Rotated(LoginResp {
                access_token,
                expires_in: access_exp,
                refresh_token: new_refresh_token,
                refresh_exp,
            }))
        })
        .await?;

    match attempt {
        RefreshAttempt::Rotated(resp) => Ok(Json(resp)),
        RefreshAttempt::Expired => Err(HttpErr::client(
            StatusCode::UNAUTHORIZED,
            REFRESH_TOKEN_EXPIRED,
        )),
        RefreshAttempt::Invalid => Err(HttpErr::client(
            StatusCode::UNAUTHORIZED,
            INVALID_REFRESH_TOKEN,
        )),
        RefreshAttempt::Reused {
            family_id,
            user_id,
            revoked_count,
        } => {
            tracing::warn!(
                event = "user.refresh_token.reuse_detected",
                user_id,
                refresh_family_id = %family_id,
                revoked_count,
                "reused refresh token detected; active token family revoked"
            );
            Err(HttpErr::client(
                StatusCode::UNAUTHORIZED,
                INVALID_REFRESH_TOKEN,
            ))
        }
    }
}

#[tracing::instrument(
    name = "user.logout",
    skip_all,
    fields(user.id = tracing::field::Empty)
)]
pub async fn logout(Json(req): Json<RefreshTokenRequest>) -> Result<StatusCode, HttpErr> {
    let token_hash = hash_refresh_token(&req.refresh_token);
    let revoked = db_conn!()
        .transaction::<Option<(i64, Uuid, usize)>, HttpErr, _>(async |conn| {
            let refresh_token = RefreshTokens::select_by_token_hash(&token_hash, conn)
                .await
                .internal(DB_ERROR)?;
            let Some(refresh_token) = refresh_token else {
                // 注销保持幂等，不通过响应暴露 refresh token 是否曾经存在。
                return Ok(None);
            };
            RefreshTokens::lock_family(refresh_token.family_id, conn)
                .await
                .internal(DB_ERROR)?;
            let revoked_count = RefreshTokens::revoke_active_family(refresh_token.family_id, conn)
                .await
                .internal(DB_ERROR)?;
            Ok(Some((
                refresh_token.user_id,
                refresh_token.family_id,
                revoked_count,
            )))
        })
        .await?;

    if let Some((user_id, family_id, revoked_count)) = revoked {
        tracing::Span::current().record("user.id", user_id);
        tracing::info!(
            event = "user.logout.completed",
            user_id,
            refresh_family_id = %family_id,
            revoked_count,
            "current login session revoked"
        );
    } else {
        tracing::info!(
            event = "user.logout.idempotent",
            "logout requested for an unknown refresh token"
        );
    }

    Ok(StatusCode::NO_CONTENT)
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
async fn mint_refresh_token(
    user_id: i64,
    family_id: Option<Uuid>,
    conn: &mut DieselConn,
) -> Result<(String, i64), HttpErr> {
    let refresh_ttl = config().refresh_ttl;
    let ttl = Duration::seconds(refresh_ttl);
    let expires_at = Utc::now() + ttl;

    let raw = generate_refresh_token();
    let token_hash = hash_refresh_token(&raw);
    let family_id = family_id.unwrap_or_else(Uuid::now_v7);

    let new = NewRefreshTokens {
        user_id,
        token_hash: &token_hash,
        family_id,
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

#[tracing::instrument(
    name = "user.current.get",
    skip_all,
    fields(user.id = auth.user_id, auth.role = %auth.role)
)]
pub async fn get_current_user(auth: AuthInfo) -> Result<Json<CurrentUserResp>, HttpErr> {
    // 身份只从鉴权中间件注入的上下文读取，客户端不能指定 user_id，确保该接口
    // 永远只返回当前登录用户自己的资料。
    let user = User::select_by_user_id(auth.user_id, &mut db_conn!())
        .await
        .internal(DB_ERROR)?
        .client(StatusCode::NOT_FOUND, USER_NOT_EXIST)?;
    tracing::info!(
        event = "user.current.loaded",
        user_id = user.user_id,
        role = %user.role,
        "current user profile loaded"
    );

    Ok(Json(to_current_user_resp(user)))
}

#[tracing::instrument(
    name = "user.avatar.update",
    skip_all,
    fields(user.id = auth.user_id, media.asset_id = %req.avatar_asset_id)
)]
pub async fn update_current_user_avatar(
    auth: AuthInfo,
    Json(req): Json<UpdateUserAvatarRequest>,
) -> Result<Json<CurrentUserResp>, HttpErr> {
    let avatar_asset_id = req.avatar_asset_id.trim().to_string();
    if avatar_asset_id.is_empty() {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }

    let mut conn = db_conn!();
    // 头像只能引用当前用户已经完成上传的图片，不能借用其他用户的媒体资源，
    // 也不能引用仍处于 pending 状态的对象。
    let assets = MediaAsset::select_uploaded_by_ids_for_owner(
        std::slice::from_ref(&avatar_asset_id),
        auth.user_id,
        &mut conn,
    )
    .await
    .internal(DB_ERROR)?;
    let asset = assets
        .first()
        .filter(|asset| asset.mime_type.starts_with("image/"))
        .ok_or_else(|| HttpErr::client(StatusCode::NOT_FOUND, MEDIA_NOT_FOUND))?;

    let user = User::update_avatar_asset_id(auth.user_id, &asset.asset_id, &mut conn)
        .await
        .internal(DB_ERROR)?;
    tracing::info!(
        event = "user.avatar.updated",
        user_id = user.user_id,
        media_asset_id = %asset.asset_id,
        "current user avatar updated"
    );

    Ok(Json(to_current_user_resp(user)))
}

fn to_current_user_resp(user: User) -> CurrentUserResp {
    CurrentUserResp {
        user_id: user.user_id.to_string(),
        username: user.username,
        account: user.account,
        role: user.role,
        avatar_asset_id: user.avatar_asset_id,
    }
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
        .protected(
            Method::GET,
            "/me/profile",
            USER_PROFILE_READ_PERMISSION,
            get(get_current_user),
        )
        .protected(
            Method::PATCH,
            "/me/profile/avatar",
            USER_PROFILE_UPDATE_PERMISSION,
            patch(update_current_user_avatar),
        )
        .public(Method::POST, "/user/login", post(login))
        .public(Method::POST, "/user/logout", post(logout))
        .public(Method::POST, "/user/refresh_token", post(refresh))
        .public(Method::POST, "/user/register", post(register))
        .public(Method::POST, "/user/verify_code", post(verify_code))
        .with_state(state)
}
