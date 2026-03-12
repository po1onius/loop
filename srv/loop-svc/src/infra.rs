pub mod notify;

use deadpool_redis::{Config, Pool as RedisPool, Runtime};
use diesel_async::AsyncPgConnection;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::pooled_connection::deadpool::Pool;
use nacos_sdk::api::config::{
    ConfigChangeListener, ConfigResponse, ConfigService, ConfigServiceBuilder,
};
use nacos_sdk::api::constants;
use nacos_sdk::api::props::ClientProps;
use std::sync::OnceLock;
use std::sync::{Arc, LazyLock};
use tera::Tera;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, fmt};

use crate::config::CONFIG;

pub static TERA: LazyLock<Tera> = LazyLock::new(|| Tera::new("static/templates/**/*").unwrap());
pub static PG_POOL: OnceLock<Pool<AsyncPgConnection>> = OnceLock::new();
pub static REDIS_POOL: OnceLock<RedisPool> = OnceLock::new();

pub fn redis_init(conn_cfg: &str) {
    let cfg = Config::from_url(conn_cfg);
    let pool = cfg.create_pool(Some(Runtime::Tokio1)).unwrap();
    _ = REDIS_POOL.set(pool);
}

pub fn init_db(conn_cfg: &str) {
    let config = AsyncDieselConnectionManager::<diesel_async::AsyncPgConnection>::new(conn_cfg);
    let pool = Pool::builder(config).build().unwrap();
    _ = PG_POOL.set(pool);
}

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

pub fn init_log() -> WorkerGuard {
    let file_appender = tracing_appender::rolling::hourly("log", "prefix.log");
    let (file_layer, guard) = tracing_appender::non_blocking(file_appender);
    let subscriber = tracing_subscriber::registry()
        .with(
            EnvFilter::new("debug")
                .add_directive("h2=off".parse().unwrap())
                .add_directive("nacos_sdk=off".parse().unwrap())
                .add_directive("tower=off".parse().unwrap())
                .add_directive("hyper_util=off".parse().unwrap()),
        )
        .with(
            fmt::Layer::new()
                .with_writer(std::io::stdout)
                .with_file(true)
                .with_line_number(true),
        )
        .with(
            fmt::Layer::new()
                .with_writer(file_layer)
                .with_ansi(false)
                .with_file(true)
                .with_line_number(true),
        );
    tracing::subscriber::set_global_default(subscriber).unwrap();
    tracing::debug!("log test");
    guard
}
