use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "dto.ts")]
pub struct LoginRequest {
    pub account: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "dto.ts")]
pub struct LoginResp {
    pub access_token: String,
    pub expires_in: i64,
    pub refresh_token: String,
    pub refresh_expires_in: i64,
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "dto.ts")]
pub struct SetTokenRequest {
    pub user_id: String,
    pub num: i64,
}
