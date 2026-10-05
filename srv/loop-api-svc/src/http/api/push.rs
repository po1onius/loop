use crate::{
    db_conn,
    http::{AppRoutes, AppState, AuthInfo, HttpErr, ResultExt, err_key::*},
};
use axum::{
    Json,
    extract::Path,
    routing::{delete, put},
};
use diesel_async::AsyncConnection;
use http::{Method, StatusCode};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct RegisterDevice {
    platform: String,
    token: String,
}

#[tracing::instrument(skip_all, fields(user_id=auth.user_id, %installation_id))]
pub async fn register(
    auth: AuthInfo,
    Path(installation_id): Path<Uuid>,
    Json(req): Json<RegisterDevice>,
) -> Result<StatusCode, HttpErr> {
    if installation_id.is_nil()
        || !matches!(req.platform.as_str(), "android" | "ios")
        || req.token.is_empty()
        || req.token.len() > 4096
    {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    db_conn!()
        .transaction::<(), HttpErr, _>(async |conn| {
            loop_svc_model::push::register(
                installation_id,
                auth.user_id,
                &req.platform,
                &req.token,
                conn,
            )
            .await
            .internal(DB_ERROR)
        })
        .await?;
    tracing::info!(event="push.device.registered", user_id=auth.user_id, %installation_id, platform=%req.platform, "push device registered");
    Ok(StatusCode::NO_CONTENT)
}

pub async fn unregister(auth: AuthInfo, Path(id): Path<Uuid>) -> Result<StatusCode, HttpErr> {
    loop_svc_model::push::unregister(id, auth.user_id, &mut db_conn!())
        .await
        .internal(DB_ERROR)?;
    tracing::info!(event="push.device.unregistered", user_id=auth.user_id, installation_id=%id, "push device unregistered");
    Ok(StatusCode::NO_CONTENT)
}

pub fn route(state: AppState) -> AppRoutes {
    AppRoutes::<AppState>::new()
        .protected(
            Method::PUT,
            "/me/push-devices/{installation_id}",
            "conversation.read",
            put(register),
        )
        .protected(
            Method::DELETE,
            "/me/push-devices/{installation_id}",
            "conversation.read",
            delete(unregister),
        )
        .with_state(state)
}
