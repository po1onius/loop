use std::sync::Arc;

use nacos_sdk::api::config::{ConfigChangeListener, ConfigResponse, ConfigServiceBuilder};
use nacos_sdk::api::constants;
use nacos_sdk::api::props::ClientProps;

use crate::config::CONFIG;

pub async fn init_nacos() -> anyhow::Result<()> {
    let client_props = ClientProps::new()
        .server_addr(constants::DEFAULT_SERVER_ADDR)
        // .remote_grpc_port(9838)
        .namespace("public")
        .app_name("loop")
        .auth_username("admin")
        .auth_password("admin");

    let config_service = ConfigServiceBuilder::new(client_props.clone())
        .enable_auth_plugin_http()
        .build()
        .await?;
    let config_resp = config_service
        .get_config("todo-data-id".to_string(), "LOVE".to_string())
        .await;
    match config_resp {
        Ok(config_resp) => tracing::info!("get the config {}", config_resp),
        Err(err) => tracing::error!("get the config {:?}", err),
    }

    let _listen = config_service
        .add_listener(
            "todo-data-id".to_string(),
            "LOVE".to_string(),
            std::sync::Arc::new(SimpleConfigChangeListener {}),
        )
        .await;
    match _listen {
        Ok(_) => tracing::info!("listening the config success"),
        Err(err) => tracing::error!("listen config error {:?}", err),
    }
    Ok(())
}

struct SimpleConfigChangeListener;

impl ConfigChangeListener for SimpleConfigChangeListener {
    fn notify(&self, config_resp: ConfigResponse) {
        tracing::info!("listen the config={}", config_resp);
        CONFIG.rcu(|cfg| {
            serde_json::from_str(&config_resp.content())
                .map_or_else(|_| cfg.to_owned(), |v| Arc::new(v))
        });
    }
}
