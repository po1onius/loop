pub mod notify;

use std::sync::{Arc, LazyLock};

use nacos_sdk::api::config::{
    ConfigChangeListener, ConfigResponse, ConfigService, ConfigServiceBuilder,
};
use nacos_sdk::api::constants;
use nacos_sdk::api::props::ClientProps;
use tera::Tera;

use crate::config::CONFIG;

pub static TERA: LazyLock<Tera> = LazyLock::new(|| Tera::new("static/templates/**/*").unwrap());

pub async fn nacos_run() -> ConfigService {
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
        .await
        .unwrap();

    let config_resp = config_service
        .get_config("default".to_string(), "default".to_string())
        .await
        .unwrap();
    CONFIG.rcu(|cfg| {
        toml::from_str(&config_resp.content()).map_or_else(
            |e| {
                tracing::error!("config error: {}", e);
                println!("{}", e);
                cfg.to_owned()
            },
            |v| Arc::new(v),
        )
    });

    config_service
        .add_listener(
            "default".to_string(),
            "default".to_string(),
            std::sync::Arc::new(SimpleConfigChangeListener),
        )
        .await
        .unwrap();

    tracing::info!("listening the config success");
    config_service
}

struct SimpleConfigChangeListener;

impl ConfigChangeListener for SimpleConfigChangeListener {
    fn notify(&self, config_resp: ConfigResponse) {
        tracing::info!("listen the config={}", config_resp);
        CONFIG.rcu(|cfg| {
            toml::from_str(&config_resp.content()).map_or_else(|_| cfg.to_owned(), |v| Arc::new(v))
        });
    }
}
