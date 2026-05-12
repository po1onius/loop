use anyhow::Context;
use arc_swap::ArcSwap;
use nacos_sdk::api::{
    config::{ConfigChangeListener, ConfigResponse, ConfigService, ConfigServiceBuilder},
    constants,
    props::ClientProps,
};
use serde::de::DeserializeOwned;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct NacosConfig {
    pub server_addr: String,
    pub namespace: String,
    pub app_name: String,
    pub auth_username: String,
    pub auth_password: String,
    pub data_id: String,
    pub group: String,
}

impl NacosConfig {
    pub fn from_env() -> Self {
        Self {
            server_addr: env_or("LOOP_NACOS_ADDR", constants::DEFAULT_SERVER_ADDR),
            namespace: env_or("LOOP_NACOS_NAMESPACE", "public"),
            app_name: env_or("LOOP_NACOS_APP_NAME", "loop"),
            auth_username: env_or("LOOP_NACOS_USERNAME", "admin"),
            auth_password: env_or("LOOP_NACOS_PASSWORD", "admin"),
            data_id: env_or("LOOP_NACOS_DATA_ID", "default"),
            group: env_or("LOOP_NACOS_GROUP", "default"),
        }
    }
}

pub async fn run_toml_config<T>(
    target: &'static ArcSwap<T>,
    cfg: NacosConfig,
) -> anyhow::Result<ConfigService>
where
    T: DeserializeOwned + Send + Sync + 'static,
{
    let client_props = ClientProps::new()
        .server_addr(cfg.server_addr)
        .namespace(cfg.namespace)
        .app_name(cfg.app_name)
        .auth_username(cfg.auth_username)
        .auth_password(cfg.auth_password);

    let config_service = ConfigServiceBuilder::new(client_props)
        .enable_auth_plugin_http()
        .build()
        .await
        .context("failed to build nacos config service")?;

    let config_resp = config_service
        .get_config(cfg.data_id.clone(), cfg.group.clone())
        .await
        .context("failed to load initial nacos config")?;
    apply_toml_config(target, &config_resp.content(), "initial nacos config")?;

    config_service
        .add_listener(
            cfg.data_id,
            cfg.group,
            Arc::new(TomlConfigChangeListener { target }),
        )
        .await
        .context("failed to register nacos config listener")?;

    tracing::info!("nacos config listener registered");
    Ok(config_service)
}

fn apply_toml_config<T>(target: &ArcSwap<T>, content: &str, source: &str) -> anyhow::Result<()>
where
    T: DeserializeOwned,
{
    let next = toml::from_str(content).with_context(|| format!("failed to parse {source}"))?;
    target.store(Arc::new(next));
    Ok(())
}

struct TomlConfigChangeListener<T: 'static> {
    target: &'static ArcSwap<T>,
}

impl<T> ConfigChangeListener for TomlConfigChangeListener<T>
where
    T: DeserializeOwned + Send + Sync + 'static,
{
    fn notify(&self, config_resp: ConfigResponse) {
        tracing::info!("received nacos config change");
        if let Err(err) = apply_toml_config(self.target, &config_resp.content(), "nacos update") {
            tracing::error!(error = %err, "failed to apply nacos config update");
        }
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}
