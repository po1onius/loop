mod config;
mod http;
mod infra;

use crate::{
    config::CONFIG,
    http::{AppState, middleware::auth, route},
    infra::nacos_run,
};
use axum::{Router, middleware};
use jsonwebtoken::{DecodingKey, EncodingKey};
use srv_common::{http::http_serve, infra::init_db};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _nacos = nacos_run().await;
    init_db(&CONFIG.load().pg_conn);

    let state = AppState {
        jwt_dec: DecodingKey::from_rsa_pem(CONFIG.load().crypto.jwt_rsa_pub_key.as_bytes())
            .unwrap(),
        jwt_enc: EncodingKey::from_rsa_pem(CONFIG.load().crypto.jwt_rsa_pri_key.as_bytes())
            .unwrap(),
    };

    let app = Router::new()
        .nest("/loop", route(state))
        .layer(middleware::from_fn_with_state(state, auth));

    http_serve(app, 3000).await;
    Ok(())
}
