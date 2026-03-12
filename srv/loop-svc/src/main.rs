mod config;
mod http;
mod infra;

use std::sync::Arc;

use crate::{
    config::CONFIG,
    http::{AppState, middleware::auth, route},
    infra::{init_db, init_log, nacos_run, redis_init},
};
use axum::{Router, middleware};
use jsonwebtoken::{DecodingKey, EncodingKey};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _guard = init_log();
    let _nacos = nacos_run().await;
    init_db(&CONFIG.load().pg_conn);
    redis_init(&CONFIG.load().redis_conn);

    let state = AppState {
        jwt_dec: Arc::new(
            DecodingKey::from_rsa_pem(CONFIG.load().crypto.jwt_rsa_pub_key.as_bytes()).unwrap(),
        ),
        jwt_enc: Arc::new(
            EncodingKey::from_rsa_pem(CONFIG.load().crypto.jwt_rsa_pri_key.as_bytes()).unwrap(),
        ),
    };

    let app = Router::new()
        .nest("/loop", route(state.clone()))
        .layer(middleware::from_fn_with_state(state, auth));

    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", 3000))
        .await
        .unwrap();
    tracing::info!("service start");
    axum::serve(listener, app).await.unwrap();
    Ok(())
}
