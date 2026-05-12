use arc_swap::ArcSwap;
use loop_infra::mail::EmailConfig;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Crypto {
    pub jwt_rsa_pri_key: String,
    pub jwt_rsa_pub_key: String,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct SMS {}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Perm {
    pub perm_ver: u32,
    pub role_perm: HashMap<String, Vec<String>>,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Config {
    pub crypto: Crypto,
    pub access_ttl: i64,
    pub refresh_ttl: i64,

    pub email: Option<EmailConfig>,
    pub sms: Option<SMS>,

    pub perm: Perm,

    pub whith_list_api: Vec<String>,
}

pub static CONFIG: LazyLock<ArcSwap<Config>> =
    LazyLock::new(|| ArcSwap::new(Arc::new(Config::default())));

#[derive(Clone, Debug)]
pub struct InfraConfig {
    pub pg_conn: String,
    pub redis_conn: String,
}

impl InfraConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            pg_conn: first_non_empty_env(&["LOOP_PG_CONN", "DATABASE_URL"])?,
            redis_conn: first_non_empty_env(&["LOOP_REDIS_CONN", "REDIS_URL"])?,
        })
    }
}

fn first_non_empty_env(keys: &[&str]) -> anyhow::Result<String> {
    keys.iter()
        .find_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "missing required environment variable; set one of: {}",
                keys.join(", ")
            )
        })
}
