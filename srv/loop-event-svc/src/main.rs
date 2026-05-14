mod config;
mod http;
mod service;

use std::{process::ExitCode, sync::Arc};

use crate::{
    config::{CONFIG, Config, InfraConfig},
    http::{AppState, middleware::auth, route},
};
use anyhow::Context;
use axum::{Router, http::HeaderName, middleware, routing::get};
use jsonwebtoken::{DecodingKey, EncodingKey};
use loop_infra::{
    db::init_pg_pool,
    observability::{
        ObservabilityConfig, extract_trace_context, init as init_observability, metrics_handler,
        record_http_metrics,
    },
    redis::init_redis_pool,
};
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};
use tracing::Level;

const SERVICE_NAME: &str = "loop-event-svc";

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
            error.chain = %chain,
            error.source = ?err,
            "service exited with error"
        );
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

async fn run() -> anyhow::Result<()> {
    let infra_config = InfraConfig::from_env()?;
    CONFIG.store(Arc::new(Config::from_env()?));
    init_pg_pool(&infra_config.pg_conn)?;
    init_redis_pool(&infra_config.redis_conn)?;

    let cfg = CONFIG.load();

    let state = AppState {
        jwt_dec: Arc::new(
            DecodingKey::from_rsa_pem(cfg.crypto.jwt_rsa_pub_key.as_bytes())
                .context("failed to parse jwt public key")?,
        ),
        jwt_enc: Arc::new(
            EncodingKey::from_rsa_pem(cfg.crypto.jwt_rsa_pri_key.as_bytes())
                .context("failed to parse jwt private key")?,
        ),
    };
    drop(cfg);

    let protected_app = Router::new()
        .nest("/loop", route(state.clone()))
        .layer(middleware::from_fn_with_state(state, auth));

    let request_id_header = HeaderName::from_static("x-request-id");
    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .merge(protected_app)
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
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        );

    let http_addr = std::env::var("LOOP_HTTP_ADDR")
        .ok()
        .map(|addr| addr.trim().to_string())
        .filter(|addr| !addr.is_empty())
        .unwrap_or_else(|| "127.0.0.1:3000".to_string());
    let listener = tokio::net::TcpListener::bind(&http_addr)
        .await
        .context("failed to bind http listener")?;
    tracing::info!(service.name = SERVICE_NAME, %http_addr, "service start");
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
        "error.chain": chain,
        "error.source": format!("{err:?}"),
    });
    eprintln!("{line}");
}

fn format_error_chain(err: &anyhow::Error) -> String {
    err.chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ")
}
