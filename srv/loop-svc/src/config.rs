use arc_swap::ArcSwap;
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
pub struct SMTP {
    pub sender: String,
    pub token: String,
    pub domain: String,
}

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Email {
    pub from: String,
    pub smtp: SMTP,
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

    pub pg_conn: String,
    pub redis_conn: String,

    pub email: Option<Email>,
    pub sms: Option<SMS>,

    pub perm: Perm,
}

pub static CONFIG: LazyLock<ArcSwap<Config>> =
    LazyLock::new(|| ArcSwap::new(Arc::new(Config::default())));
