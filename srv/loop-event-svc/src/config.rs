use anyhow::{Context, bail};
use loop_infra::mail::{EmailConfig, SmtpConfig};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, sync::OnceLock};

#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(default)]
pub struct Crypto {
    pub jwt_rsa_pri_key: String,
    pub jwt_rsa_pub_key: String,
}

#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(default)]
pub struct Sms {}

#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(default)]
pub struct Perm {
    pub perm_ver: u32,
    pub role_perm: HashMap<String, Vec<String>>,
}

#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(default)]
pub struct Config {
    pub crypto: Crypto,
    pub access_ttl: i64,
    pub refresh_ttl: i64,

    pub email: Option<EmailConfig>,
    pub sms: Option<Sms>,

    pub perm: Perm,
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
    #[tracing::instrument(name = "config.service.load", skip_all)]
    pub fn from_env() -> anyhow::Result<Self> {
        let mut cfg = load_base_config()?;
        apply_env_overrides(&mut cfg)?;
        cfg.validate()?;
        Ok(cfg)
    }

    #[tracing::instrument(name = "config.service.validate", skip_all)]
    fn validate(&self) -> anyhow::Result<()> {
        ensure_non_empty("crypto.jwt_rsa_pri_key", &self.crypto.jwt_rsa_pri_key)?;
        ensure_non_empty("crypto.jwt_rsa_pub_key", &self.crypto.jwt_rsa_pub_key)?;
        if self.access_ttl <= 0 {
            bail!("access_ttl must be positive");
        }
        if self.refresh_ttl <= 0 {
            bail!("refresh_ttl must be positive");
        }

        if let Some(email) = &self.email {
            ensure_non_empty("email.from", &email.from)?;
            ensure_non_empty("email.smtp.sender", &email.smtp.sender)?;
            ensure_non_empty("email.smtp.token", &email.smtp.token)?;
            ensure_non_empty("email.smtp.domain", &email.smtp.domain)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct InfraConfig {
    pub pg_conn: String,
    pub redis_conn: String,
}

impl InfraConfig {
    #[tracing::instrument(name = "config.infra.load", skip_all)]
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            pg_conn: first_non_empty_env(&["LOOP_PG_CONN", "DATABASE_URL"])?,
            redis_conn: first_non_empty_env(&["LOOP_REDIS_CONN", "REDIS_URL"])?,
        })
    }
}

#[tracing::instrument(
    name = "config.base.load",
    skip_all,
    fields(config.file = tracing::field::Empty)
)]
fn load_base_config() -> anyhow::Result<Config> {
    let Some(path) = first_non_empty_env_opt(&["LOOP_CONFIG_FILE"]) else {
        tracing::info!(
            event = "config.base.load",
            config_source = "environment",
            "loading service config from environment"
        );
        return Ok(Config::default());
    };
    tracing::Span::current().record("config.file", tracing::field::display(&path));
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

#[tracing::instrument(name = "config.env.apply", skip_all)]
fn apply_env_overrides(cfg: &mut Config) -> anyhow::Result<()> {
    if let Some(value) = value_from_env_or_file(
        &["LOOP_JWT_RSA_PRI_KEY", "LOOP_JWT_RSA_PRIVATE_KEY"],
        &["LOOP_JWT_RSA_PRI_KEY_FILE", "LOOP_JWT_RSA_PRIVATE_KEY_FILE"],
    )? {
        cfg.crypto.jwt_rsa_pri_key = value;
    }
    if let Some(value) = value_from_env_or_file(
        &["LOOP_JWT_RSA_PUB_KEY", "LOOP_JWT_RSA_PUBLIC_KEY"],
        &["LOOP_JWT_RSA_PUB_KEY_FILE", "LOOP_JWT_RSA_PUBLIC_KEY_FILE"],
    )? {
        cfg.crypto.jwt_rsa_pub_key = value;
    }
    if let Some(value) = first_non_empty_env_opt(&["LOOP_ACCESS_TTL"]) {
        cfg.access_ttl = parse_env_i64("LOOP_ACCESS_TTL", &value)?;
    }
    if let Some(value) = first_non_empty_env_opt(&["LOOP_REFRESH_TTL"]) {
        cfg.refresh_ttl = parse_env_i64("LOOP_REFRESH_TTL", &value)?;
    }
    if let Some(value) = first_non_empty_env_opt(&["LOOP_PERM_VER"]) {
        cfg.perm.perm_ver = parse_env_u32("LOOP_PERM_VER", &value)?;
    }
    if let Some(value) = first_non_empty_env_opt(&["LOOP_ROLE_PERM_JSON"]) {
        cfg.perm.role_perm =
            serde_json::from_str(&value).context("failed to parse LOOP_ROLE_PERM_JSON")?;
    }

    apply_email_env_overrides(cfg)?;
    Ok(())
}

#[tracing::instrument(name = "config.email_env.apply", skip_all)]
fn apply_email_env_overrides(cfg: &mut Config) -> anyhow::Result<()> {
    let email_env_keys = [
        "LOOP_EMAIL_FROM",
        "LOOP_SMTP_SENDER",
        "LOOP_SMTP_TOKEN",
        "LOOP_SMTP_TOKEN_FILE",
        "LOOP_SMTP_DOMAIN",
    ];
    if !email_env_keys.iter().any(|key| env_has_value(key)) {
        return Ok(());
    }

    let email = cfg.email.get_or_insert_with(|| EmailConfig {
        from: String::new(),
        smtp: SmtpConfig::default(),
    });
    if let Some(value) = first_non_empty_env_opt(&["LOOP_EMAIL_FROM"]) {
        email.from = value;
    }
    if let Some(value) = first_non_empty_env_opt(&["LOOP_SMTP_SENDER"]) {
        email.smtp.sender = value;
    }
    if let Some(value) = value_from_env_or_file(&["LOOP_SMTP_TOKEN"], &["LOOP_SMTP_TOKEN_FILE"])? {
        email.smtp.token = value;
    }
    if let Some(value) = first_non_empty_env_opt(&["LOOP_SMTP_DOMAIN"]) {
        email.smtp.domain = value;
    }
    Ok(())
}

fn value_from_env_or_file(
    value_keys: &[&str],
    file_keys: &[&str],
) -> anyhow::Result<Option<String>> {
    let value = first_non_empty_env_opt(value_keys);
    let file = first_non_empty_env_opt(file_keys);
    match (value, file) {
        (Some(_), Some(_)) => bail!(
            "set either one of [{}] or one of [{}], not both",
            value_keys.join(", "),
            file_keys.join(", ")
        ),
        (Some(value), None) => Ok(Some(value)),
        (None, Some(path)) => read_non_empty_file(&path).map(Some),
        (None, None) => Ok(None),
    }
}

#[tracing::instrument(
    name = "config.secret_file.read",
    skip_all,
    fields(config.file = %path)
)]
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

fn first_non_empty_env(keys: &[&str]) -> anyhow::Result<String> {
    first_non_empty_env_opt(keys).ok_or_else(|| {
        anyhow::anyhow!(
            "missing required environment variable; set one of: {}",
            keys.join(", ")
        )
    })
}

fn first_non_empty_env_opt(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn env_has_value(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
}
