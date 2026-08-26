use anyhow::{Context, anyhow};
use meilisearch_sdk::client::Client;
use std::{
    fmt::{self, Debug, Formatter},
    sync::OnceLock,
};

static MEILISEARCH_CLIENT: OnceLock<Client> = OnceLock::new();

/// Meilisearch 属于部署基础设施，服务地址与管理密钥只允许通过环境变量或
/// `*_FILE` Secret 注入。业务索引名、可搜索字段等策略由共享搜索模块维护，
/// API 负责查询，worker 负责写入。
#[derive(Clone, Default)]
pub struct MeilisearchConfig {
    pub url: String,
    pub api_key: String,
}

impl Debug for MeilisearchConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("MeilisearchConfig")
            .field("url", &self.url)
            .field("api_key", &"<redacted>")
            .finish()
    }
}

impl MeilisearchConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.url.trim().is_empty() {
            anyhow::bail!("search.url is required");
        }
        if self.api_key.trim().is_empty() {
            anyhow::bail!("search.api_key is required");
        }
        Ok(())
    }
}

#[tracing::instrument(name = "infra.meilisearch.client.init", skip_all)]
pub fn init_meilisearch_client(config: MeilisearchConfig) -> anyhow::Result<()> {
    config.validate()?;
    let url = config.url.trim_end_matches('/').to_string();
    let client = Client::new(url.clone(), Some(config.api_key))
        .context("failed to build Meilisearch client")?;
    MEILISEARCH_CLIENT
        .set(client)
        .map_err(|_| anyhow!("Meilisearch client has already been initialized"))?;
    tracing::info!(
        event = "infra.meilisearch.client.initialized",
        meilisearch_url = %url,
        "Meilisearch client initialized"
    );
    Ok(())
}

pub fn meilisearch_client() -> anyhow::Result<&'static Client> {
    MEILISEARCH_CLIENT
        .get()
        .context("Meilisearch client is not initialized")
}
