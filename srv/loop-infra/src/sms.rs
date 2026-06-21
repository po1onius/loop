use anyhow::{Context, anyhow};
use std::sync::OnceLock;

static SMS_CONFIG: OnceLock<SmsConfig> = OnceLock::new();

/// Placeholder for SMS provider settings. Keeping the type in infra lets
/// business services depend on a stable SMS boundary before a provider is wired.
#[derive(Clone, Debug, Default)]
pub struct SmsConfig {}

#[tracing::instrument(name = "infra.sms.init", skip_all)]
pub fn init_sms_config(config: SmsConfig) -> anyhow::Result<()> {
    SMS_CONFIG
        .set(config)
        .map_err(|_| anyhow!("SMS config has already been initialized"))
}

pub fn sms_config() -> anyhow::Result<&'static SmsConfig> {
    SMS_CONFIG.get().context("SMS config is not initialized")
}
