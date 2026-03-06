pub mod user;

use crate::{
    config::CONFIG,
    http::user::{login, refresh},
};
use axum::{Router, routing::post};
use jsonwebtoken::{DecodingKey, EncodingKey};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    pub exp: usize,

    pub user_id: i64,
    pub perm_ver: u32,
    pub perms: Vec<u32>,
}

#[derive(Clone)]
pub struct AppState {
    pub jwt_enc: EncodingKey,
    pub jwt_dec: DecodingKey,
}

pub fn route() -> Router {
    let state = AppState {
        jwt_dec: DecodingKey::from_rsa_pem(CONFIG.load().crypto.jwt_rsa_pub_key.as_bytes())
            .unwrap(),
        jwt_enc: EncodingKey::from_rsa_pem(CONFIG.load().crypto.jwt_rsa_pri_key.as_bytes())
            .unwrap(),
    };
    Router::new()
        .route("/user/login", post(login))
        .route("/user/refresh_token", post(refresh))
        .with_state(state)
}
