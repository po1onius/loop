#[cfg(any(
    feature = "db",
    feature = "redis",
    feature = "mail",
    feature = "storage",
    feature = "search",
    feature = "rabbitmq"
))]
use anyhow::Context;
#[cfg(any(
    feature = "db",
    feature = "redis",
    feature = "mail",
    feature = "storage",
    feature = "search",
    feature = "rabbitmq"
))]
use std::fs;

#[cfg(feature = "mail")]
use crate::mail::EmailConfig;
#[cfg(feature = "rabbitmq")]
use crate::rabbitmq::RabbitMqConfig;
#[cfg(feature = "search")]
use crate::search::MeilisearchConfig;
#[cfg(feature = "sms")]
use crate::sms::SmsConfig;
#[cfg(feature = "storage")]
use crate::storage::S3StorageConfig;

/// Aggregates infrastructure settings for the components enabled by crate
/// features.
///
/// 基础设施配置只从环境变量或 `*_FILE` Secret 文件读取，不读取业务 TOML。
/// 这样可以避免部署环境参数和业务策略配置混在同一个文件里。
#[derive(Clone, Debug, Default)]
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
    #[cfg(feature = "search")]
    pub search: MeilisearchConfig,
    #[cfg(feature = "rabbitmq")]
    pub rabbitmq: RabbitMqConfig,
}

impl InfraConfig {
    /// Load infrastructure settings from env vars or `*_FILE` secret files.
    pub fn from_env() -> anyhow::Result<Self> {
        let mut cfg = InfraConfig::default();
        apply_env_overrides(&mut cfg)?;
        cfg.validate()?;
        Ok(cfg)
    }

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
        #[cfg(feature = "search")]
        self.search
            .validate()
            .context("invalid search configuration")?;
        #[cfg(feature = "rabbitmq")]
        self.rabbitmq
            .validate()
            .context("invalid RabbitMQ configuration")?;
        Ok(())
    }

    /// Initialize all enabled infrastructure singletons once during service
    /// bootstrap. If a feature is enabled, its configuration is required and
    /// invalid settings fail fast before the HTTP server starts.
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
        #[cfg(feature = "search")]
        crate::search::init_meilisearch_client(self.search)?;
        #[cfg(feature = "rabbitmq")]
        crate::rabbitmq::init_rabbitmq_config(self.rabbitmq)?;
        Ok(())
    }
}

fn apply_env_overrides(cfg: &mut InfraConfig) -> anyhow::Result<()> {
    let _ = cfg;

    #[cfg(feature = "db")]
    {
        cfg.pg_conn = build_pg_conn_from_env()?;
    }
    #[cfg(feature = "redis")]
    {
        cfg.redis_conn = env_required("REDIS_URL")?;

        tracing::info!(
            event = "infra.config.redis.loaded",
            "redis url loaded from deployment configuration"
        );
    }

    #[cfg(feature = "mail")]
    apply_email_env_overrides(cfg)?;
    #[cfg(feature = "storage")]
    apply_storage_env_overrides(cfg)?;
    #[cfg(feature = "search")]
    apply_search_env_overrides(cfg)?;
    #[cfg(feature = "rabbitmq")]
    apply_rabbitmq_env_overrides(cfg)?;

    Ok(())
}

#[cfg(feature = "rabbitmq")]
fn apply_rabbitmq_env_overrides(cfg: &mut InfraConfig) -> anyhow::Result<()> {
    cfg.rabbitmq.url = env_or_file("RABBITMQ_URL", "RABBITMQ_URL_FILE")?
        .ok_or_else(|| anyhow::anyhow!("RABBITMQ_URL or RABBITMQ_URL_FILE is required"))?;
    tracing::info!(
        event = "infra.config.rabbitmq.loaded",
        "RabbitMQ configuration loaded from deployment configuration"
    );
    Ok(())
}

#[cfg(feature = "search")]
fn apply_search_env_overrides(cfg: &mut InfraConfig) -> anyhow::Result<()> {
    cfg.search.url = env_required("MEILISEARCH_URL")?;
    cfg.search.api_key = env_or_file("MEILISEARCH_API_KEY", "MEILISEARCH_API_KEY_FILE")?
        .ok_or_else(|| {
            anyhow::anyhow!("MEILISEARCH_API_KEY or MEILISEARCH_API_KEY_FILE is required")
        })?;
    tracing::info!(
        event = "infra.config.meilisearch.loaded",
        "Meilisearch configuration loaded from deployment configuration"
    );
    Ok(())
}

#[cfg(feature = "db")]
fn build_pg_conn_from_env() -> anyhow::Result<String> {
    // 业务服务只读取最终连接串。不同部署形态需要的 host、端口、账号拼接，
    // 放在 Compose、Makefile 或 K8s Secret/ConfigMap 这类部署层处理。
    let value = env_required("DATABASE_URL")?;

    tracing::info!(
        event = "infra.config.postgres.loaded",
        "postgres database url loaded from deployment configuration"
    );

    Ok(value)
}

#[cfg(any(feature = "db", feature = "redis", feature = "search"))]
fn env_required(key: &str) -> anyhow::Result<String> {
    env_opt(key).ok_or_else(|| anyhow::anyhow!("{key} is required"))
}

#[cfg(feature = "mail")]
fn apply_email_env_overrides(cfg: &mut InfraConfig) -> anyhow::Result<()> {
    if let Some(value) = env_opt("LOOP_EMAIL_FROM") {
        cfg.email.from = value;
    }
    if let Some(value) = env_opt("LOOP_SMTP_SENDER") {
        cfg.email.smtp.sender = value;
    }
    if let Some(value) = env_or_file("LOOP_SMTP_TOKEN", "LOOP_SMTP_TOKEN_FILE")? {
        cfg.email.smtp.token = value;
    }
    if let Some(value) = env_opt("LOOP_SMTP_DOMAIN") {
        cfg.email.smtp.domain = value;
    }
    Ok(())
}

#[cfg(feature = "storage")]
fn apply_storage_env_overrides(cfg: &mut InfraConfig) -> anyhow::Result<()> {
    if let Some(value) = env_opt("LOOP_S3_BUCKET") {
        cfg.storage.bucket = value;
    }
    if let Some(value) = env_opt("LOOP_S3_REGION") {
        cfg.storage.region = value;
    }
    if let Some(value) = env_opt("LOOP_S3_ENDPOINT_URL") {
        cfg.storage.endpoint_url = Some(value);
    }
    if let Some(value) = env_opt("LOOP_S3_PRESIGN_ENDPOINT_URL") {
        cfg.storage.presign_endpoint_url = Some(value);
    }
    if let Some(value) = env_opt("LOOP_S3_PUBLIC_BASE_URL") {
        cfg.storage.public_base_url = Some(value);
    }
    if let Some(value) = env_opt("LOOP_S3_KEY_PREFIX") {
        cfg.storage.key_prefix = value;
    }
    if let Some(value) = env_or_file("LOOP_S3_ACCESS_KEY_ID", "LOOP_S3_ACCESS_KEY_ID_FILE")? {
        cfg.storage.access_key_id = value;
    }
    if let Some(value) = env_or_file(
        "LOOP_S3_SECRET_ACCESS_KEY",
        "LOOP_S3_SECRET_ACCESS_KEY_FILE",
    )? {
        cfg.storage.secret_access_key = value;
    }
    if let Some(value) = env_or_file("LOOP_S3_SESSION_TOKEN", "LOOP_S3_SESSION_TOKEN_FILE")? {
        cfg.storage.session_token = Some(value);
    }
    if let Some(value) = env_opt("LOOP_S3_FORCE_PATH_STYLE") {
        cfg.storage.force_path_style = parse_env_bool("LOOP_S3_FORCE_PATH_STYLE", &value)?;
    }
    Ok(())
}

#[cfg(any(
    feature = "db",
    feature = "redis",
    feature = "mail",
    feature = "storage",
    feature = "search",
    feature = "rabbitmq"
))]
fn env_or_file(value_key: &str, file_key: &str) -> anyhow::Result<Option<String>> {
    let value = env_opt(value_key);
    let file = env_opt(file_key);
    match (value, file) {
        (Some(_), Some(_)) => anyhow::bail!("set either {value_key} or {file_key}, not both"),
        (Some(value), None) => Ok(Some(value)),
        (None, Some(path)) => read_non_empty_file(&path).map(Some),
        (None, None) => Ok(None),
    }
}

#[cfg(any(
    feature = "db",
    feature = "redis",
    feature = "mail",
    feature = "storage",
    feature = "search",
    feature = "rabbitmq"
))]
fn read_non_empty_file(path: &str) -> anyhow::Result<String> {
    let value =
        fs::read_to_string(path).with_context(|| format!("failed to read secret file: {path}"))?;
    if value.trim().is_empty() {
        anyhow::bail!("secret file is empty: {path}");
    }
    Ok(value.trim().to_string())
}

#[cfg(feature = "storage")]
fn parse_env_bool(key: &str, value: &str) -> anyhow::Result<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "on" => Ok(true),
        "0" | "false" | "no" | "n" | "off" => Ok(false),
        _ => anyhow::bail!("failed to parse {key} as bool"),
    }
}

#[cfg(any(feature = "db", feature = "redis", feature = "search"))]
fn ensure_non_empty(name: &str, value: &str) -> anyhow::Result<()> {
    if value.trim().is_empty() {
        anyhow::bail!("{name} is required");
    }
    Ok(())
}

#[cfg(any(
    feature = "db",
    feature = "redis",
    feature = "mail",
    feature = "storage",
    feature = "search",
    feature = "rabbitmq"
))]
fn env_opt(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
