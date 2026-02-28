pub mod account;

use crate::{
    config::CONFIG,
    http::account::{login, refresh},
};
use axum::{Router, routing::post};
use jsonwebtoken::{DecodingKey, EncodingKey};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    pub exp: usize,

    pub user_id: Uuid,
    pub perm_ver: u32,
    pub perms: Vec<u32>,
}

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
