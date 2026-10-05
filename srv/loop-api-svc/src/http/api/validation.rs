use crate::{
    db_conn,
    http::{HttpErr, ResultExt, err_key::*},
};
use http::StatusCode;
use loop_svc_model::{DieselConn, event::MediaAsset};
use std::collections::HashSet;
use uuid::Uuid;
pub(super) async fn validate_image_assets(
    user_id: i64,
    asset_ids: &[String],
) -> Result<(), HttpErr> {
    validate_image_assets_with_conn(user_id, asset_ids, &mut db_conn!()).await
}

pub(super) async fn validate_image_assets_with_conn(
    user_id: i64,
    asset_ids: &[String],
    conn: &mut DieselConn,
) -> Result<(), HttpErr> {
    if asset_ids.is_empty() {
        return Ok(());
    }
    let assets = MediaAsset::select_uploaded_by_ids_for_owner(asset_ids, user_id, conn)
        .await
        .internal(DB_ERROR)?;
    if assets.len() != asset_ids.len()
        || assets
            .iter()
            .any(|asset| !asset.mime_type.starts_with("image/"))
    {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, MEDIA_NOT_FOUND));
    }
    Ok(())
}

pub(super) fn normalize_asset_ids(
    asset_ids: Vec<String>,
    max_count: usize,
) -> Result<Vec<String>, HttpErr> {
    let mut seen = HashSet::new();
    let normalized = asset_ids
        .into_iter()
        .map(|asset_id| asset_id.trim().to_string())
        .filter(|asset_id| !asset_id.is_empty())
        .filter(|asset_id| seen.insert(asset_id.clone()))
        .collect::<Vec<_>>();
    if normalized.len() > max_count {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    Ok(normalized)
}

pub(super) fn normalize_limit(value: Option<i64>, default: i64, max: i64) -> Result<i64, HttpErr> {
    let value = value.unwrap_or(default);
    if value <= 0 || value > max {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    Ok(value)
}

pub(super) fn parse_uuid(value: &str) -> Result<Uuid, HttpErr> {
    Uuid::parse_str(value.trim())
        .map_err(|_| HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT))
}
