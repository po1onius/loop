use anyhow::{Context, anyhow};
use lapin::{Connection, ConnectionProperties};
use std::{
    fmt::{self, Debug, Formatter},
    sync::OnceLock,
};

static RABBITMQ_CONFIG: OnceLock<RabbitMqConfig> = OnceLock::new();

/// RabbitMQ URL 通常包含用户名和密码，因此 Debug 输出只显示是否已经配置，
/// 不能把完整连接串写入日志或错误上下文。
#[derive(Clone, Default)]
pub struct RabbitMqConfig {
    pub url: String,
}

impl Debug for RabbitMqConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("RabbitMqConfig")
            .field("url", &"<redacted>")
            .finish()
    }
}

impl RabbitMqConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        let url = self.url.trim();
        if url.is_empty() {
            anyhow::bail!("rabbitmq.url is required");
        }
        if !url.starts_with("amqp://") && !url.starts_with("amqps://") {
            anyhow::bail!("rabbitmq.url must use amqp:// or amqps://");
        }
        Ok(())
    }
}

pub fn init_rabbitmq_config(config: RabbitMqConfig) -> anyhow::Result<()> {
    config.validate()?;
    RABBITMQ_CONFIG
        .set(config)
        .map_err(|_| anyhow!("RabbitMQ configuration has already been initialized"))?;
    tracing::info!(
        event = "infra.rabbitmq.config_initialized",
        "RabbitMQ configuration initialized"
    );
    Ok(())
}

pub fn rabbitmq_config() -> anyhow::Result<&'static RabbitMqConfig> {
    RABBITMQ_CONFIG
        .get()
        .context("RabbitMQ configuration is not initialized")
}

#[tracing::instrument(name = "infra.rabbitmq.connect", skip_all)]
pub async fn connect(connection_name: &str) -> anyhow::Result<Connection> {
    let config = rabbitmq_config()?;
    let properties = ConnectionProperties::default().with_connection_name(connection_name.into());
    let connection = Connection::connect(&config.url, properties)
        .await
        .context("failed to connect to RabbitMQ")?;
    tracing::info!(
        event = "infra.rabbitmq.connected",
        messaging_system = "rabbitmq",
        connection_name,
        "RabbitMQ connection established"
    );
    Ok(connection)
}
