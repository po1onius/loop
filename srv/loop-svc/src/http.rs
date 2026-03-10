pub mod api;
pub mod middleware;
mod util;

use crate::{
    config::CONFIG,
    http::api::user::{self, login, refresh},
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

#[derive(Clone)]
pub struct AuthInfo {
    user_id: i64,
}

pub fn route(state: AppState) -> Router {
    Router::new().merge(user::route(state))
}

pub mod err_key {
    pub const TMR: &str = "too_many_requests";
    pub const RTE: &str = "refresh_token_expction";
    pub const VCE: &str = "verfiy_code_expired";
    pub const VCW: &str = "verfiy_code_wrong";
    pub const XE: &str = "???";
    pub const EMS: &str = "email_send_error";
}
