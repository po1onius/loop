pub mod config;
pub mod http;
pub mod service;

use std::{process::ExitCode, sync::Arc};

use anyhow::Context;
use axum::{
    Router,
    http::{HeaderName, Method, header},
    middleware,
    routing::get,
};
use jsonwebtoken::{DecodingKey, EncodingKey};
use loop_infra::config::InfraConfig;
use loop_infra::observability::{ObservabilityConfig, init as init_observability};
use loop_infra::observability::{extract_trace_context, metrics_handler, record_http_metrics};
use loop_search::EventSearchService;
use tower_http::{
    cors::{Any, CorsLayer},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};
use tracing::Level;

use crate::{
    config::{Config, config, init_config},
    http::{
        AppState,
        middleware::{AuthMiddlewareState, AuthPolicy, auth},
        route,
    },
};

pub const SERVICE_NAME: &str = "loop-api-svc";

#[tokio::main]
async fn main() -> ExitCode {
    let _observability = init_observability(ObservabilityConfig::new(
        SERVICE_NAME,
        env!("CARGO_PKG_VERSION"),
    ));
    let _observability = match _observability {
        Ok(guard) => guard,
        Err(err) => {
            log_bootstrap_error("failed to initialize observability", &err);
            return ExitCode::FAILURE;
        }
    };

    if let Err(err) = run().await {
        let chain = format_error_chain(&err);
        tracing::error!(
            event = "service.exit",
            error_kind = "bootstrap",
            error_chain = %chain,
            error_source = ?err,
            "service exited with error"
        );
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

pub fn build_app(state: AppState) -> Router {
    let routes = route(state.clone());
    let (api_router, rules) = routes.into_parts();
    let auth_state = AuthMiddlewareState::new(state, AuthPolicy::new(rules));
    let api_app = api_router.layer(middleware::from_fn_with_state(auth_state, auth));
    let api_app = if should_enable_cors() {
        api_app.layer(cors_layer())
    } else {
        api_app
    };

    let request_id_header = HeaderName::from_static("x-request-id");
    Router::new()
        .route("/metrics", get(metrics_handler))
        .merge(api_app)
        .layer(middleware::from_fn(record_http_metrics))
        .layer(PropagateRequestIdLayer::new(request_id_header.clone()))
        .layer(SetRequestIdLayer::new(request_id_header, MakeRequestUuid))
        .layer(middleware::from_fn(extract_trace_context))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(
                    DefaultMakeSpan::new()
                        .level(Level::INFO)
                        .include_headers(false),
                )
                .on_response(DefaultOnResponse::new().level(Level::DEBUG)),
        )
}

fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
}

fn should_enable_cors() -> bool {
    cfg!(debug_assertions)
        || std::env::var("LOOP_ENV").is_ok_and(|env| env.eq_ignore_ascii_case("local"))
}

async fn run() -> anyhow::Result<()> {
    let infra_config = InfraConfig::from_env()?;
    infra_config.init()?;
    init_config(Config::from_env()?)?;

    let cfg = config();

    let event_search = Arc::new(EventSearchService::from_env()?);
    // API 仅确保查询所需索引和 settings 已就绪；存量同步与增量写入由 worker
    // 负责，避免活动发布请求等待 Meilisearch 索引任务。
    event_search
        .initialize_reader()
        .await
        .context("failed to initialize event search")?;

    let state = AppState {
        jwt_dec: Arc::new(
            DecodingKey::from_rsa_pem(cfg.crypto.jwt_rsa_pub_key.as_bytes())
                .context("failed to parse jwt public key")?,
        ),
        jwt_enc: Arc::new(
            EncodingKey::from_rsa_pem(cfg.crypto.jwt_rsa_pri_key.as_bytes())
                .context("failed to parse jwt private key")?,
        ),
        event_search,
    };

    let app = build_app(state);

    let http_addr = std::env::var("LOOP_HTTP_ADDR")
        .ok()
        .map(|addr| addr.trim().to_string())
        .filter(|addr| !addr.is_empty())
        .unwrap_or_else(|| "127.0.0.1:3000".to_string());
    let listener = tokio::net::TcpListener::bind(&http_addr)
        .await
        .context("failed to bind http listener")?;
    tracing::info!(
        event = "service.start",
        service_name = SERVICE_NAME,
        %http_addr,
        "service started on {http_addr}"
    );
    axum::serve(listener, app)
        .await
        .context("http server stopped with error")?;
    Ok(())
}

fn log_bootstrap_error(message: &str, err: &anyhow::Error) {
    let chain = format_error_chain(err);
    let line = serde_json::json!({
        "level": "ERROR",
        "message": message,
        "event": "service.bootstrap_error",
        "error_kind": "bootstrap",
        "error_chain": chain,
        "error_source": format!("{err:?}"),
    });
    eprintln!("{line}");
}

fn format_error_chain(err: &anyhow::Error) -> String {
    err.chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ")
}
