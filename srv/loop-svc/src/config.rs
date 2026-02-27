use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

#[derive(Serialize, Deserialize)]
pub struct Crypto {
    pub jwt_rsa_pri_key: String,
    pub jwt_rsa_pub_key: String,
}

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub crypto: Crypto,
    pub jwt_expire_duration: usize,
    pub pg_conn: String,
    pub redis_conn: String,
}

pub static CONFIG: LazyLock<Config> = LazyLock::new(config_init);

pub fn config_init() -> Config {
    let cfg_path = "config.toml";
    let config_str = std::fs::read_to_string(cfg_path).expect("read config error");
    toml::from_str(config_str.as_str()).expect("load toml config error")
}
