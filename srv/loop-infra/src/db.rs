use anyhow::{Context, anyhow};
use diesel_async::{
    AsyncPgConnection,
    pooled_connection::{AsyncDieselConnectionManager, deadpool::Pool},
};
use std::sync::OnceLock;

pub type PgPool = Pool<AsyncPgConnection>;

pub static PG_POOL: OnceLock<PgPool> = OnceLock::new();

pub fn build_pg_pool(conn_cfg: &str) -> anyhow::Result<PgPool> {
    let config = AsyncDieselConnectionManager::<AsyncPgConnection>::new(conn_cfg);
    Pool::builder(config)
        .build()
        .context("failed to build postgres connection pool")
}

pub fn init_pg_pool(conn_cfg: &str) -> anyhow::Result<()> {
    let pool = build_pg_pool(conn_cfg)?;
    PG_POOL
        .set(pool)
        .map_err(|_| anyhow!("postgres connection pool has already been initialized"))
}

pub fn pg_pool() -> anyhow::Result<&'static PgPool> {
    PG_POOL
        .get()
        .context("postgres connection pool is not initialized")
}
