mod config;
mod http;
mod infra;

use serde::Serialize;
use srv_common::{http::http_serve, infra::init_db};

use crate::{http::route, infra::init_nacos};

#[derive(Debug, Serialize)]
struct MeResp {
    user_id: String,
    email: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_db("");
    init_nacos().await?;

    let app = route();

    http_serve(app, 3000).await;
    Ok(())
}
