use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, sync::OnceLock};

const MAX_MEDIA_PRESIGN_EXPIRES_SECS: u64 = 60 * 60 * 24 * 7;

#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(default)]
pub struct Crypto {
    #[serde(skip)]
    pub jwt_rsa_pri_key: String,
    #[serde(skip)]
    pub jwt_rsa_pub_key: String,
}

#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(default)]
pub struct Perm {
    pub perm_ver: u32,
    pub role_perm: HashMap<String, Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(default)]
pub struct MediaConfig {
    pub presign_expires_secs: u64,
    pub max_upload_bytes: i64,
    pub allowed_mime_types: Vec<String>,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            presign_expires_secs: 15 * 60,
            max_upload_bytes: 10 * 1024 * 1024,
            allowed_mime_types: vec![
                "image/jpeg".to_string(),
                "image/png".to_string(),
                "image/webp".to_string(),
                "image/gif".to_string(),
            ],
        }
    }
}

#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(default)]
pub struct Config {
    pub crypto: Crypto,
    pub access_ttl: i64,
    pub refresh_ttl: i64,

    pub perm: Perm,
    pub media: MediaConfig,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

pub fn init_config(cfg: Config) -> anyhow::Result<()> {
    CONFIG
        .set(cfg)
        .map_err(|_| anyhow::anyhow!("service config has already been initialized"))
}

pub fn config() -> &'static Config {
    CONFIG
        .get()
        .expect("service config is not initialized; call init_config before using config")
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let mut cfg = load_base_config()?;
        apply_env_overrides(&mut cfg)?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> anyhow::Result<()> {
        ensure_non_empty("crypto.jwt_rsa_pri_key", &self.crypto.jwt_rsa_pri_key)?;
        ensure_non_empty("crypto.jwt_rsa_pub_key", &self.crypto.jwt_rsa_pub_key)?;
        if self.access_ttl <= 0 {
            bail!("access_ttl must be positive");
        }
        if self.refresh_ttl <= 0 {
            bail!("refresh_ttl must be positive");
        }
        self.media.validate()?;

        Ok(())
    }
}

impl MediaConfig {
    fn validate(&self) -> anyhow::Result<()> {
        if self.presign_expires_secs == 0
            || self.presign_expires_secs > MAX_MEDIA_PRESIGN_EXPIRES_SECS
        {
            bail!(
                "media.presign_expires_secs must be between 1 and {MAX_MEDIA_PRESIGN_EXPIRES_SECS}"
            );
        }
        if self.max_upload_bytes <= 0 {
            bail!("media.max_upload_bytes must be positive");
        }
        if self.allowed_mime_types.is_empty() {
            bail!("media.allowed_mime_types must not be empty");
        }
        for mime_type in &self.allowed_mime_types {
            ensure_non_empty("media.allowed_mime_types[]", mime_type)?;
        }
        Ok(())
    }

    pub fn is_mime_type_allowed(&self, mime_type: &str) -> bool {
        self.allowed_mime_types
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(mime_type))
    }
}

fn load_base_config() -> anyhow::Result<Config> {
    let Some(path) = env_opt("LOOP_CONFIG_FILE") else {
        tracing::info!(
            event = "config.base.load",
            config_source = "environment",
            "loading service config from environment"
        );
        return Ok(Config::default());
    };
    tracing::info!(
        event = "config.base.load",
        config_source = "file",
        config_file = %path,
        "loading service config file {path}"
    );
    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read config file: {path}"))?;
    toml::from_str(&content).with_context(|| format!("failed to parse config file: {path}"))
}

fn apply_env_overrides(cfg: &mut Config) -> anyhow::Result<()> {
    if let Some(value) = env_or_file("LOOP_JWT_RSA_PRI_KEY", "LOOP_JWT_RSA_PRI_KEY_FILE")? {
        cfg.crypto.jwt_rsa_pri_key = value;
    }
    if let Some(value) = env_or_file("LOOP_JWT_RSA_PUB_KEY", "LOOP_JWT_RSA_PUB_KEY_FILE")? {
        cfg.crypto.jwt_rsa_pub_key = value;
    }
    if let Some(value) = env_opt("LOOP_ACCESS_TTL") {
        cfg.access_ttl = parse_env_i64("LOOP_ACCESS_TTL", &value)?;
    }
    if let Some(value) = env_opt("LOOP_REFRESH_TTL") {
        cfg.refresh_ttl = parse_env_i64("LOOP_REFRESH_TTL", &value)?;
    }
    if let Some(value) = env_opt("LOOP_PERM_VER") {
        cfg.perm.perm_ver = parse_env_u32("LOOP_PERM_VER", &value)?;
    }
    if let Some(value) = env_opt("LOOP_ROLE_PERM_JSON") {
        cfg.perm.role_perm =
            serde_json::from_str(&value).context("failed to parse LOOP_ROLE_PERM_JSON")?;
    }

    Ok(())
}

fn env_or_file(value_key: &str, file_key: &str) -> anyhow::Result<Option<String>> {
    let value = env_opt(value_key);
    let file = env_opt(file_key);
    match (value, file) {
        (Some(_), Some(_)) => bail!("set either {value_key} or {file_key}, not both"),
        (Some(value), None) => Ok(Some(value)),
        (None, Some(path)) => read_non_empty_file(&path).map(Some),
        (None, None) => Ok(None),
    }
}

fn read_non_empty_file(path: &str) -> anyhow::Result<String> {
    let value =
        fs::read_to_string(path).with_context(|| format!("failed to read secret file: {path}"))?;
    if value.trim().is_empty() {
        bail!("secret file is empty: {path}");
    }
    Ok(value.trim().to_string())
}

fn parse_env_i64(key: &str, value: &str) -> anyhow::Result<i64> {
    value
        .parse()
        .with_context(|| format!("failed to parse {key} as i64"))
}

fn parse_env_u32(key: &str, value: &str) -> anyhow::Result<u32> {
    value
        .parse()
        .with_context(|| format!("failed to parse {key} as u32"))
}

fn ensure_non_empty(name: &str, value: &str) -> anyhow::Result<()> {
    if value.trim().is_empty() {
        bail!("{name} is required");
    }
    Ok(())
}

fn env_opt(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
