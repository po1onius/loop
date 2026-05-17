use anyhow::Context;
use serde::Deserialize;
use std::fs;

#[cfg(feature = "mail")]
use crate::mail::EmailConfig;
#[cfg(feature = "sms")]
use crate::sms::SmsConfig;
#[cfg(feature = "storage")]
use crate::storage::S3StorageConfig;

/// Aggregates infrastructure settings for the components enabled by crate
/// features. The same TOML file can contain both service and infrastructure
/// sections because unknown fields are ignored by each side's deserializer.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct InfraConfig {
    #[cfg(feature = "db")]
    pub pg_conn: String,
    #[cfg(feature = "redis")]
    pub redis_conn: String,
    #[cfg(feature = "mail")]
    pub email: EmailConfig,
    #[cfg(feature = "sms")]
    pub sms: SmsConfig,
    #[cfg(feature = "storage")]
    pub storage: S3StorageConfig,
}

impl InfraConfig {
    /// Load ordinary values from `LOOP_CONFIG_FILE`, then overlay deployment
    /// secrets and environment-specific overrides from env vars or `*_FILE`.
    #[tracing::instrument(name = "infra.config.load", skip_all)]
    pub fn from_env() -> anyhow::Result<Self> {
        let mut cfg = load_base_config()?;
        apply_env_overrides(&mut cfg)?;
        cfg.validate()?;
        Ok(cfg)
    }

    #[tracing::instrument(name = "infra.config.validate", skip_all)]
    pub fn validate(&self) -> anyhow::Result<()> {
        #[cfg(feature = "db")]
        ensure_non_empty("infra.pg_conn", &self.pg_conn)?;
        #[cfg(feature = "redis")]
        ensure_non_empty("infra.redis_conn", &self.redis_conn)?;

        #[cfg(feature = "mail")]
        self.email
            .validate()
            .context("invalid email configuration")?;
        #[cfg(feature = "storage")]
        self.storage
            .validate()
            .context("invalid storage configuration")?;
        Ok(())
    }

    /// Initialize all enabled infrastructure singletons once during service
    /// bootstrap. If a feature is enabled, its configuration is required and
    /// invalid settings fail fast before the HTTP server starts.
    #[tracing::instrument(name = "infra.components.init", skip_all)]
    pub fn init(self) -> anyhow::Result<()> {
        #[cfg(feature = "db")]
        crate::db::init_pg_pool(&self.pg_conn)?;
        #[cfg(feature = "redis")]
        crate::redis::init_redis_pool(&self.redis_conn)?;

        #[cfg(feature = "mail")]
        crate::mail::init_email_config(self.email)?;

        #[cfg(feature = "sms")]
        crate::sms::init_sms_config(self.sms)?;

        #[cfg(feature = "storage")]
        crate::storage::init_s3_storage(self.storage)?;
        Ok(())
    }
}

#[tracing::instrument(
    name = "infra.config.base.load",
    skip_all,
    fields(config.file = tracing::field::Empty)
)]
fn load_base_config() -> anyhow::Result<InfraConfig> {
    let Some(path) = first_non_empty_env_opt(&["LOOP_CONFIG_FILE"]) else {
        tracing::info!(
            event = "infra.config.base.load",
            config_source = "environment",
            "loading infrastructure config from environment"
        );
        return Ok(InfraConfig::default());
    };

    tracing::Span::current().record("config.file", tracing::field::display(&path));
    tracing::info!(
        event = "infra.config.base.load",
        config_source = "file",
        config_file = %path,
        "loading infrastructure config file {path}"
    );
    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read config file: {path}"))?;
    toml::from_str(&content).with_context(|| format!("failed to parse config file: {path}"))
}

#[tracing::instrument(name = "infra.config.env.apply", skip_all)]
fn apply_env_overrides(cfg: &mut InfraConfig) -> anyhow::Result<()> {
    let _ = cfg;

    #[cfg(feature = "db")]
    {
        if let Some(value) = first_non_empty_env_opt(&["LOOP_PG_CONN", "DATABASE_URL"]) {
            cfg.pg_conn = value;
        }
    }
    #[cfg(feature = "redis")]
    {
        if let Some(value) = first_non_empty_env_opt(&["LOOP_REDIS_CONN", "REDIS_URL"]) {
            cfg.redis_conn = value;
        }
    }

    #[cfg(feature = "mail")]
    apply_email_env_overrides(cfg)?;
    #[cfg(feature = "storage")]
    apply_storage_env_overrides(cfg)?;

    Ok(())
}

#[cfg(feature = "mail")]
#[tracing::instrument(name = "infra.config.email_env.apply", skip_all)]
fn apply_email_env_overrides(cfg: &mut InfraConfig) -> anyhow::Result<()> {
    if let Some(value) = first_non_empty_env_opt(&["LOOP_EMAIL_FROM"]) {
        cfg.email.from = value;
    }
    if let Some(value) = first_non_empty_env_opt(&["LOOP_SMTP_SENDER"]) {
        cfg.email.smtp.sender = value;
    }
    if let Some(value) = value_from_env_or_file(&["LOOP_SMTP_TOKEN"], &["LOOP_SMTP_TOKEN_FILE"])? {
        cfg.email.smtp.token = value;
    }
    if let Some(value) = first_non_empty_env_opt(&["LOOP_SMTP_DOMAIN"]) {
        cfg.email.smtp.domain = value;
    }
    Ok(())
}

#[cfg(feature = "storage")]
#[tracing::instrument(name = "infra.config.storage_env.apply", skip_all)]
fn apply_storage_env_overrides(cfg: &mut InfraConfig) -> anyhow::Result<()> {
    if let Some(value) = first_non_empty_env_opt(&["LOOP_S3_BUCKET"]) {
        cfg.storage.bucket = value;
    }
    if let Some(value) = first_non_empty_env_opt(&["LOOP_S3_REGION"]) {
        cfg.storage.region = value;
    }
    if let Some(value) = first_non_empty_env_opt(&["LOOP_S3_ENDPOINT_URL"]) {
        cfg.storage.endpoint_url = Some(value);
    }
    if let Some(value) = first_non_empty_env_opt(&["LOOP_S3_PUBLIC_BASE_URL"]) {
        cfg.storage.public_base_url = Some(value);
    }
    if let Some(value) = first_non_empty_env_opt(&["LOOP_S3_KEY_PREFIX"]) {
        cfg.storage.key_prefix = value;
    }
    if let Some(value) =
        value_from_env_or_file(&["LOOP_S3_ACCESS_KEY_ID"], &["LOOP_S3_ACCESS_KEY_ID_FILE"])?
    {
        cfg.storage.access_key_id = value;
    }
    if let Some(value) = value_from_env_or_file(
        &["LOOP_S3_SECRET_ACCESS_KEY"],
        &["LOOP_S3_SECRET_ACCESS_KEY_FILE"],
    )? {
        cfg.storage.secret_access_key = value;
    }
    if let Some(value) =
        value_from_env_or_file(&["LOOP_S3_SESSION_TOKEN"], &["LOOP_S3_SESSION_TOKEN_FILE"])?
    {
        cfg.storage.session_token = Some(value);
    }
    if let Some(value) = first_non_empty_env_opt(&["LOOP_S3_FORCE_PATH_STYLE"]) {
        cfg.storage.force_path_style = parse_env_bool("LOOP_S3_FORCE_PATH_STYLE", &value)?;
    }
    if let Some(value) = first_non_empty_env_opt(&["LOOP_S3_PRESIGN_EXPIRES_SECS"]) {
        cfg.storage.presign_expires_secs = parse_env_u64("LOOP_S3_PRESIGN_EXPIRES_SECS", &value)?;
    }
    if let Some(value) = first_non_empty_env_opt(&["LOOP_S3_MAX_UPLOAD_BYTES"]) {
        cfg.storage.max_upload_bytes = parse_env_i64("LOOP_S3_MAX_UPLOAD_BYTES", &value)?;
    }
    if let Some(value) = first_non_empty_env_opt(&["LOOP_S3_ALLOWED_MIME_TYPES"]) {
        cfg.storage.allowed_mime_types = parse_env_list(&value);
    }
    Ok(())
}

#[cfg(any(feature = "mail", feature = "storage"))]
fn value_from_env_or_file(
    value_keys: &[&str],
    file_keys: &[&str],
) -> anyhow::Result<Option<String>> {
    let value = first_non_empty_env_opt(value_keys);
    let file = first_non_empty_env_opt(file_keys);
    match (value, file) {
        (Some(_), Some(_)) => anyhow::bail!(
            "set either one of [{}] or one of [{}], not both",
            value_keys.join(", "),
            file_keys.join(", ")
        ),
        (Some(value), None) => Ok(Some(value)),
        (None, Some(path)) => read_non_empty_file(&path).map(Some),
        (None, None) => Ok(None),
    }
}

#[cfg(any(feature = "mail", feature = "storage"))]
#[tracing::instrument(
    name = "infra.config.secret_file.read",
    skip_all,
    fields(config.file = %path)
)]
fn read_non_empty_file(path: &str) -> anyhow::Result<String> {
    let value =
        fs::read_to_string(path).with_context(|| format!("failed to read secret file: {path}"))?;
    if value.trim().is_empty() {
        anyhow::bail!("secret file is empty: {path}");
    }
    Ok(value.trim().to_string())
}

#[cfg(feature = "storage")]
fn parse_env_i64(key: &str, value: &str) -> anyhow::Result<i64> {
    value
        .parse()
        .with_context(|| format!("failed to parse {key} as i64"))
}

#[cfg(feature = "storage")]
fn parse_env_u64(key: &str, value: &str) -> anyhow::Result<u64> {
    value
        .parse()
        .with_context(|| format!("failed to parse {key} as u64"))
}

#[cfg(feature = "storage")]
fn parse_env_bool(key: &str, value: &str) -> anyhow::Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "on" => Ok(true),
        "0" | "false" | "no" | "n" | "off" => Ok(false),
        _ => anyhow::bail!("failed to parse {key} as bool"),
    }
}

#[cfg(feature = "storage")]
fn parse_env_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

#[cfg(any(feature = "db", feature = "redis"))]
fn ensure_non_empty(name: &str, value: &str) -> anyhow::Result<()> {
    if value.trim().is_empty() {
        anyhow::bail!("{name} is required");
    }
    Ok(())
}

fn first_non_empty_env_opt(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        std::env::var(key)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}
