use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, LazyLock};

#[derive(Serialize, Deserialize, Default)]
pub struct Crypto {
    pub jwt_rsa_pri_key: String,
    pub jwt_rsa_pub_key: String,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Config {
    pub crypto: Crypto,
    pub access_ttl: i64,
    pub refresh_ttl: i64,

    pub pg_conn: String,
    pub redis_conn: String,
}

pub static CONFIG: LazyLock<ArcSwap<Config>> =
    LazyLock::new(|| ArcSwap::new(Arc::new(Config::default())));
