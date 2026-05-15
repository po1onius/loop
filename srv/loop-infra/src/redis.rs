use anyhow::{Context, anyhow};
use deadpool_redis::{Config, Pool as RedisPool, Runtime};
use std::sync::OnceLock;

pub static REDIS_POOL: OnceLock<RedisPool> = OnceLock::new();

#[tracing::instrument(
    name = "infra.redis.pool.build",
    skip_all,
    fields(db.system = "redis")
)]
pub fn build_redis_pool(conn_cfg: &str) -> anyhow::Result<RedisPool> {
    let cfg = Config::from_url(conn_cfg);
    cfg.create_pool(Some(Runtime::Tokio1))
        .context("failed to build redis connection pool")
}

#[tracing::instrument(
    name = "infra.redis.pool.init",
    skip_all,
    fields(db.system = "redis")
)]
pub fn init_redis_pool(conn_cfg: &str) -> anyhow::Result<()> {
    let pool = build_redis_pool(conn_cfg)?;
    REDIS_POOL
        .set(pool)
        .map_err(|_| anyhow!("redis connection pool has already been initialized"))
}

pub fn redis_pool() -> anyhow::Result<&'static RedisPool> {
    REDIS_POOL
        .get()
        .context("redis connection pool is not initialized")
}
