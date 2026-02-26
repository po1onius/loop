use deadpool::{Runtime, managed::Pool as RedisPool};
use deadpool_redis::{Config, Connection, Manager};
use diesel_async::AsyncPgConnection;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::pooled_connection::deadpool::Pool;
use std::sync::OnceLock;

pub static PG_POOL: OnceLock<Pool<AsyncPgConnection>> = OnceLock::new();
pub static REDIS_POOL: OnceLock<RedisPool<Manager, Connection>> = OnceLock::new();

pub fn redis_init(conn_cfg: &str) {
    let cfg = Config::from_url(conn_cfg);
    let pool = cfg
        .create_pool(Some(Runtime::Tokio1))
        .expect("redis init error");
    _ = REDIS_POOL.set(pool);
}

pub fn init_db(conn_cfg: &str) {
    let config = AsyncDieselConnectionManager::<diesel_async::AsyncPgConnection>::new(conn_cfg);
    let pool = Pool::builder(config).build().expect("db init error");
    _ = PG_POOL.set(pool);
}
