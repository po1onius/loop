pub mod account;

use axum::{Router, routing::post};
use jsonwebtoken::{DecodingKey, EncodingKey};
use std::sync::Arc;

use crate::{
    config::CONFIG,
    http::account::{login, refresh},
};

pub struct AppState {
    pub jwt_enc: EncodingKey,
    pub jwt_dec: DecodingKey,
}

pub fn route() -> Router {
    let state = AppState {
        jwt_dec: DecodingKey::from_rsa_pem(CONFIG.crypto.jwt_rsa_pub_key.as_bytes()).unwrap(),

        jwt_enc: EncodingKey::from_rsa_pem(CONFIG.crypto.jwt_rsa_pri_key.as_bytes()).unwrap(),
    };
    Router::new()
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .with_state(Arc::new(state))
}
