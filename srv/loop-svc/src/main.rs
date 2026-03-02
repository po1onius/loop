mod config;
mod http;
mod infra;

use crate::{http::route, infra::init_nacos};
use srv_common::{http::http_serve, infra::init_db};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_db("");
    init_nacos().await?;

    let app = route();

    http_serve(app, 3000).await;
    Ok(())
}
