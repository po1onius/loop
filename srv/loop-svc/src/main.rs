mod config;
mod http;
mod infra;

use crate::{config::CONFIG, http::route, infra::nacos_run};
use srv_common::{http::http_serve, infra::init_db};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _nacos = nacos_run().await;
    init_db(&CONFIG.load().pg_conn);

    let app = route();

    http_serve(app, 3000).await;
    Ok(())
}
