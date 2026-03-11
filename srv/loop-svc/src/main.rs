mod config;
mod http;
mod infra;

use std::sync::Arc;

use crate::{
    config::CONFIG,
    http::{AppState, http_serve, middleware::auth, route},
    infra::{init_db, nacos_run},
};
use axum::{Router, middleware};
use jsonwebtoken::{DecodingKey, EncodingKey};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _nacos = nacos_run().await;
    init_db(&CONFIG.load().pg_conn);

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

    http_serve(app, 3000).await;
    Ok(())
}
