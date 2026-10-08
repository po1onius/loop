use crate::http::{AppRoutes, AppState};
use anyhow::{Context, bail};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{Method, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use loop_dto::{ConvertCoordinatesRequest, ConvertCoordinatesResp};
use serde::Deserialize;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

/// JS key is public; the security code is only attached to server-to-AMap requests.
pub struct MapService {
    key: String,
    security_code: String,
    web_service_key: String,
    client: amap_http::Client,
}

impl MapService {
    pub fn from_env() -> anyhow::Result<Self> {
        let key = std::env::var("LOOP_AMAP_JS_KEY")
            .unwrap_or_default()
            .trim()
            .to_owned();
        let security_code = std::env::var("LOOP_AMAP_SECURITY_CODE")
            .unwrap_or_default()
            .trim()
            .to_owned();
        let web_service_key = std::env::var("LOOP_AMAP_WEB_SERVICE_KEY")
            .unwrap_or_default()
            .trim()
            .to_owned();
        if key.is_empty() != security_code.is_empty() {
            bail!("LOOP_AMAP_JS_KEY and LOOP_AMAP_SECURITY_CODE must be configured together");
        }
        if !key.bytes().all(|b| b.is_ascii_alphanumeric()) {
            bail!("LOOP_AMAP_JS_KEY must be alphanumeric");
        }
        if !web_service_key.bytes().all(|b| b.is_ascii_alphanumeric()) {
            bail!("LOOP_AMAP_WEB_SERVICE_KEY must be alphanumeric");
        }
        let client = amap_http::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .redirect(amap_http::redirect::Policy::none())
            .build()
            .context("failed to initialize AMap HTTP client")?;
        tracing::info!(
            event = "maps.configured",
            enabled = !key.is_empty(),
            coordinate_conversion_enabled = !web_service_key.is_empty(),
            "initialized map services"
        );
        Ok(Self {
            key,
            security_code,
            web_service_key,
            client,
        })
    }
}

pub fn route(state: AppState) -> AppRoutes {
    AppRoutes::new()
        .public(Method::GET, "/maps/picker", get(picker))
        .public(Method::GET, "/maps/picker.js", get(picker_script))
        .public(
            Method::POST,
            "/maps/coordinates/convert",
            post(convert_coordinates),
        )
        .public(Method::GET, "/maps/_AMapService/{*path}", get(proxy))
        .with_state(state)
}

#[derive(Deserialize)]
struct AMapConversion {
    status: String,
    infocode: Option<String>,
    locations: Option<String>,
}

// POST keeps user coordinates out of access-log URLs. Neither request nor upstream
// response/error bodies may be logged: they can contain coordinates and credentials.
#[tracing::instrument(name = "maps.convert", skip_all)]
async fn convert_coordinates(
    State(state): State<AppState>,
    Json(point): Json<ConvertCoordinatesRequest>,
) -> Result<impl IntoResponse, (StatusCode, &'static str)> {
    if !valid_coordinates(point.latitude, point.longitude) {
        tracing::warn!(
            event = "maps.convert.invalid_coordinates",
            "invalid coordinate range"
        );
        return Err((StatusCode::BAD_REQUEST, "定位坐标无效"));
    }
    if state.maps.web_service_key.is_empty() {
        tracing::warn!(
            event = "maps.convert.unconfigured",
            "Web service key is missing"
        );
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "距离服务尚未配置，请稍后重试",
        ));
    }
    let started = Instant::now();
    let locations = format!("{},{}", point.longitude, point.latitude);
    let mut response = state
        .maps
        .client
        .get("https://restapi.amap.com/v3/assistant/coordinate/convert")
        .query(&[
            ("key", state.maps.web_service_key.as_str()),
            ("locations", locations.as_str()),
            ("coordsys", "gps"),
            ("output", "JSON"),
        ])
        .send()
        .await
        .map_err(|error| {
            tracing::warn!(
                event = "maps.convert.failed",
                timeout = error.is_timeout(),
                connect = error.is_connect(),
                elapsed_ms = started.elapsed().as_millis() as u64,
                "coordinate conversion request failed"
            );
            (StatusCode::BAD_GATEWAY, "距离计算暂不可用，请重试")
        })?;
    if !response.status().is_success() {
        tracing::warn!(
            event = "maps.convert.http_error",
            status = response.status().as_u16(),
            "coordinate conversion returned HTTP error"
        );
        return Err((StatusCode::BAD_GATEWAY, "距离计算暂不可用，请重试"));
    }
    let mut body = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) if body.len() + chunk.len() <= 4096 => body.extend_from_slice(&chunk),
            Ok(None) => break,
            _ => {
                tracing::warn!(
                    event = "maps.convert.body_failed",
                    "conversion response incomplete or too large"
                );
                return Err((StatusCode::BAD_GATEWAY, "距离计算暂不可用，请重试"));
            }
        }
    }
    let result: AMapConversion = serde_json::from_slice(&body).map_err(|_| {
        tracing::warn!(
            event = "maps.convert.invalid_response",
            "invalid conversion JSON"
        );
        (StatusCode::BAD_GATEWAY, "距离计算暂不可用，请重试")
    })?;
    if result.status != "1" {
        tracing::warn!(
            event = "maps.convert.rejected",
            infocode = result.infocode.and_then(|code| code.parse::<u32>().ok()),
            "AMap rejected conversion"
        );
        return Err((StatusCode::BAD_GATEWAY, "距离计算暂不可用，请重试"));
    }
    let point = result
        .locations
        .as_deref()
        .and_then(|value| value.split_once(','))
        .and_then(|(lng, lat)| Some((lat.parse::<f64>().ok()?, lng.parse::<f64>().ok()?)))
        .filter(|&(lat, lng)| valid_coordinates(lat, lng))
        .ok_or_else(|| {
            tracing::warn!(
                event = "maps.convert.invalid_result",
                "invalid converted coordinates"
            );
            (StatusCode::BAD_GATEWAY, "距离计算暂不可用，请重试")
        })?;
    tracing::info!(
        event = "maps.convert.completed",
        elapsed_ms = started.elapsed().as_millis() as u64,
        "coordinate conversion completed"
    );
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(ConvertCoordinatesResp {
            latitude: point.0,
            longitude: point.1,
        }),
    ))
}

fn valid_coordinates(latitude: f64, longitude: f64) -> bool {
    latitude.is_finite()
        && longitude.is_finite()
        && (-90.0..=90.0).contains(&latitude)
        && (-180.0..=180.0).contains(&longitude)
}

async fn picker(State(state): State<AppState>) -> Response {
    if state.maps.key.is_empty() {
        tracing::warn!(
            event = "maps.unconfigured",
            "map picker requested without AMap credentials"
        );
        return (StatusCode::SERVICE_UNAVAILABLE, Html("<!doctype html><html lang=\"zh-CN\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><p>地图服务尚未配置，请稍后重试。</p></html>")).into_response();
    }
    tracing::debug!(event = "maps.picker.open", "serving map picker");
    let html =
        include_str!("../../../assets/map-picker.html").replace("__AMAP_JS_KEY__", &state.maps.key);
    (
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::REFERRER_POLICY, "strict-origin-when-cross-origin"),
        ],
        Html(html),
    )
        .into_response()
}

async fn picker_script() -> impl IntoResponse {
    (
        [
            (
                header::CONTENT_TYPE,
                "application/javascript; charset=utf-8",
            ),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        include_str!("../../../assets/map-picker.js"),
    )
}

/// Only the picker services are exposed, never an arbitrary URL or client-supplied secret.
async fn proxy(
    State(state): State<AppState>,
    Path(path): Path<String>,
    Query(mut query): Query<HashMap<String, String>>,
) -> Response {
    const ALLOWED: &[&str] = &[
        "v3/place/text",
        "v3/place/around",
        "v3/place/detail",
        "v3/geocode/regeo",
        "v3/assistant/inputtips",
        "v3/assistant/coordinate/convert",
        "v3/ip",
    ];
    if state.maps.key.is_empty() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if !ALLOWED.contains(&path.as_str()) || query.get("key") != Some(&state.maps.key) {
        tracing::warn!(
            event = "maps.proxy.rejected",
            "rejected unsupported map proxy request"
        );
        return StatusCode::BAD_REQUEST.into_response();
    }
    query.insert("jscode".into(), state.maps.security_code.clone());
    let started = Instant::now();
    let result = state
        .maps
        .client
        .get(format!("https://restapi.amap.com/{path}"))
        .query(&query)
        .send()
        .await;
    let mut upstream = match result {
        Ok(response) if response.status().is_success() => response,
        Ok(response) => {
            tracing::warn!(event = "maps.proxy.failed", service = %path, status = response.status().as_u16(), "AMap returned an HTTP error");
            return StatusCode::BAD_GATEWAY.into_response();
        }
        Err(error) => {
            // reqwest errors may contain the full URL, including jscode and coordinates.
            tracing::warn!(event = "maps.proxy.failed", service = %path, timeout = error.is_timeout(), connect = error.is_connect(), "AMap request failed");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };
    let content_type = if query.contains_key("callback") {
        "application/javascript; charset=utf-8"
    } else {
        "application/json; charset=utf-8"
    };
    let mut body = Vec::new();
    loop {
        match upstream.chunk().await {
            Ok(Some(chunk)) if body.len() + chunk.len() <= 2 * 1024 * 1024 => {
                body.extend_from_slice(&chunk)
            }
            Ok(None) => break,
            _ => {
                tracing::warn!(event = "maps.proxy.body_failed", service = %path, "AMap response incomplete or too large");
                return StatusCode::BAD_GATEWAY.into_response();
            }
        }
    }
    tracing::info!(event = "maps.proxy.completed", service = %path, elapsed_ms = started.elapsed().as_millis() as u64, bytes = body.len(), "AMap request completed");
    (
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "no-store"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        body,
    )
        .into_response()
}
