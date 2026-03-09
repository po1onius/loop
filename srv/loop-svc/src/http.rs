pub mod user;
mod util;

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
    pub role: String,
    pub patch_perm: PatchPerm,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PatchPerm {
    pub ban: Vec<String>,
    pub ext: Vec<String>,
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
    Router::new().merge(user::route(state))
}

pub mod err_key {
    pub const TMR: &str = "too_many_requests";
    pub const RTE: &str = "refresh_token_expction";
    pub const VCE: &str = "verfiy_code_expired";
    pub const VCW: &str = "verfiy_code_wrong";
    pub const XE: &str = "???";
}
