use crate::{
    config::{MediaConfig, config},
    db_conn,
    http::{AppRoutes, AppState, AuthInfo, HttpErr, ResultExt, err_key::*},
};
use axum::{
    Json,
    extract::Path,
    routing::{get, post},
};
use chrono::Utc;
use diesel::result::Error as DieselError;
use http::{Method, StatusCode};
use loop_dto::{
    CompleteMediaUploadResp, CreateMediaUploadRequest, CreateMediaUploadResp, MediaAssetResp,
    MediaDownloadUrlResp, PresignedHeader,
};
use loop_infra::storage::{self, S3StorageConfig};
use loop_svc_model::event::{MediaAsset, NewMediaAsset};
use serde_json::json;
use uuid::Uuid;

const MEDIA_UPLOAD_PERMISSION: &str = "media.upload";
const MAX_MIME_TYPE_CHARS: usize = 128;

#[tracing::instrument(
    name = "media.upload_url.create",
    skip_all,
    fields(user.id = auth.user_id, media.asset_id = tracing::field::Empty)
)]
pub async fn create_upload_url(
    auth: AuthInfo,
    Json(req): Json<CreateMediaUploadRequest>,
) -> Result<(StatusCode, Json<CreateMediaUploadResp>), HttpErr> {
    let storage_config = storage_config_or_error()?;
    let media_config = &config().media;
    let mime_type = normalize_mime_type(&req.mime_type)?;
    validate_upload_request(&req, media_config, &mime_type)?;

    let asset_id = Uuid::now_v7().to_string();
    let storage_key = build_storage_key(auth.user_id, &asset_id, &mime_type, storage_config);
    let presigned =
        storage::presign_put(&storage_key, &mime_type, media_config.presign_expires_secs)
            .await
            .internal(STORAGE_ERROR)?;
    let public_url = storage::public_url(&storage_key).internal(STORAGE_ERROR)?;

    let new_asset = NewMediaAsset {
        asset_id: asset_id.clone(),
        owner_id: auth.user_id,
        storage_key: storage_key.clone(),
        mime_type: mime_type.clone(),
        byte_size: i64::from(req.byte_size),
        width: req.width,
        height: req.height,
        status: "pending".to_string(),
        variants_json: json!({
            "original": {
                "public_url": public_url,
            }
        }),
    };
    MediaAsset::insert(new_asset, &mut db_conn!())
        .await
        .internal(DB_ERROR)?;
    tracing::Span::current().record("media.asset_id", tracing::field::display(&asset_id));
    tracing::info!(
        event = "media.upload_url.created",
        media_asset_id = %asset_id,
        user_id = auth.user_id,
        storage_key = %storage_key,
        mime_type = %mime_type,
        byte_size = req.byte_size,
        "media upload URL created"
    );

    Ok((
        StatusCode::CREATED,
        Json(CreateMediaUploadResp {
            asset_id,
            storage_key,
            upload_method: presigned.method,
            upload_url: presigned.url,
            upload_headers: to_dto_headers(presigned.headers),
            expires_in: presigned.expires_in as i32,
            public_url,
        }),
    ))
}

#[tracing::instrument(
    name = "media.upload.complete",
    skip_all,
    fields(user.id = auth.user_id, media.asset_id = %asset_id)
)]
pub async fn complete_upload(
    auth: AuthInfo,
    Path(asset_id): Path<String>,
) -> Result<Json<CompleteMediaUploadResp>, HttpErr> {
    storage_config_or_error()?;
    let asset = MediaAsset::select_by_asset_id(&asset_id, &mut db_conn!())
        .await
        .internal(DB_ERROR)?
        .filter(|asset| asset.owner_id == auth.user_id)
        .ok_or_else(|| HttpErr::client(StatusCode::NOT_FOUND, MEDIA_NOT_FOUND))?;

    let head = storage::head_object(&asset.storage_key)
        .await
        .internal(STORAGE_ERROR)?;
    if head.byte_size != asset.byte_size {
        tracing::warn!(
            event = "media.upload.size_mismatch",
            media_asset_id = %asset.asset_id,
            expected_byte_size = asset.byte_size,
            actual_byte_size = head.byte_size,
            "uploaded media object size does not match registered size"
        );
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    if head
        .mime_type
        .as_deref()
        .is_some_and(|value| !value.eq_ignore_ascii_case(&asset.mime_type))
    {
        tracing::warn!(
            event = "media.upload.mime_mismatch",
            media_asset_id = %asset.asset_id,
            expected_mime_type = %asset.mime_type,
            actual_mime_type = ?head.mime_type,
            "uploaded media object content type does not match registered type"
        );
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }

    let asset = MediaAsset::mark_uploaded(&asset_id, auth.user_id, &mut db_conn!())
        .await
        .map_err(map_media_update_error)?;
    tracing::info!(
        event = "media.upload.completed",
        media_asset_id = %asset.asset_id,
        user_id = auth.user_id,
        storage_key = %asset.storage_key,
        "media upload completed"
    );

    Ok(Json(CompleteMediaUploadResp {
        asset: to_asset_resp(asset)?,
    }))
}

#[tracing::instrument(name = "media.download_url.create", skip_all, fields(media.asset_id = %asset_id))]
pub async fn download_url(
    Path(asset_id): Path<String>,
) -> Result<Json<MediaDownloadUrlResp>, HttpErr> {
    storage_config_or_error()?;
    let media_config = &config().media;
    let asset = MediaAsset::select_uploaded_by_asset_id(&asset_id, &mut db_conn!())
        .await
        .internal(DB_ERROR)?
        .ok_or_else(|| HttpErr::client(StatusCode::NOT_FOUND, MEDIA_NOT_FOUND))?;
    let presigned = storage::presign_get(&asset.storage_key, media_config.presign_expires_secs)
        .await
        .internal(STORAGE_ERROR)?;

    tracing::info!(
        event = "media.download_url.created",
        media_asset_id = %asset.asset_id,
        storage_key = %asset.storage_key,
        "media download URL created"
    );

    Ok(Json(MediaDownloadUrlResp {
        asset_id,
        download_method: presigned.method,
        download_url: presigned.url,
        download_headers: to_dto_headers(presigned.headers),
        expires_in: presigned.expires_in as i32,
    }))
}

fn storage_config_or_error() -> Result<&'static S3StorageConfig, HttpErr> {
    storage::s3_storage_config()
        .map_err(|_| HttpErr::client(StatusCode::SERVICE_UNAVAILABLE, STORAGE_UNCONFIGURED))
}

fn validate_upload_request(
    req: &CreateMediaUploadRequest,
    config: &MediaConfig,
    mime_type: &str,
) -> Result<(), HttpErr> {
    if req.byte_size <= 0 || i64::from(req.byte_size) > config.max_upload_bytes {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    if !config.is_mime_type_allowed(mime_type) {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    if req.width.is_some_and(|width| width <= 0) || req.height.is_some_and(|height| height <= 0) {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    Ok(())
}

fn normalize_mime_type(value: &str) -> Result<String, HttpErr> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty()
        || value.chars().count() > MAX_MIME_TYPE_CHARS
        || !value.contains('/')
        || value.contains(char::is_whitespace)
    {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    Ok(value)
}

fn build_storage_key(
    owner_id: i64,
    asset_id: &str,
    mime_type: &str,
    config: &S3StorageConfig,
) -> String {
    let today = Utc::now().format("%Y/%m/%d");
    let filename = format!("{asset_id}.{}", extension_for_mime_type(mime_type));
    let prefix = config.normalized_key_prefix();
    if prefix.is_empty() {
        format!("users/{owner_id}/{today}/{filename}")
    } else {
        format!("{prefix}/users/{owner_id}/{today}/{filename}")
    }
}

fn extension_for_mime_type(mime_type: &str) -> &'static str {
    match mime_type {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "bin",
    }
}

fn to_dto_headers(
    headers: Vec<loop_infra::storage::PresignedStorageHeader>,
) -> Vec<PresignedHeader> {
    headers
        .into_iter()
        .map(|header| PresignedHeader {
            name: header.name,
            value: header.value,
        })
        .collect()
}

fn to_asset_resp(asset: MediaAsset) -> Result<MediaAssetResp, HttpErr> {
    let public_url = storage::public_url(&asset.storage_key).internal(STORAGE_ERROR)?;
    Ok(MediaAssetResp {
        public_url,
        asset_id: asset.asset_id,
        mime_type: asset.mime_type,
        byte_size: asset.byte_size,
        width: asset.width,
        height: asset.height,
        status: asset.status,
        created_at: asset.created_at.to_rfc3339(),
    })
}

fn map_media_update_error(err: DieselError) -> HttpErr {
    match err {
        DieselError::NotFound => HttpErr::client(StatusCode::NOT_FOUND, MEDIA_NOT_FOUND),
        err => HttpErr::internal(DB_ERROR, err),
    }
}

pub fn route(state: AppState) -> AppRoutes {
    AppRoutes::<AppState>::new()
        .protected(
            Method::POST,
            "/media/upload_url",
            MEDIA_UPLOAD_PERMISSION,
            post(create_upload_url),
        )
        .protected(
            Method::POST,
            "/media/{asset_id}/complete",
            MEDIA_UPLOAD_PERMISSION,
            post(complete_upload),
        )
        .public(
            Method::GET,
            "/media/{asset_id}/download_url",
            get(download_url),
        )
        .with_state(state)
}
