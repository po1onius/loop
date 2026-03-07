pub mod account;

use deadpool::managed::{Object, Pool};
use diesel::QueryDsl;
use diesel_async::{
    AsyncConnection, AsyncPgConnection, RunQueryDsl,
    pooled_connection::AsyncDieselConnectionManager,
};

use crate::account::{RefreshTokens, User, users};

pub type DieselConn = Object<AsyncDieselConnectionManager<AsyncPgConnection>>;

#[test]
fn f() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        // create a new connection pool with the default config
        let config = AsyncDieselConnectionManager::<diesel_async::AsyncPgConnection>::new(
            "postgresql://localhost:5132/loop?user=srus&password=wdnmd",
        );
        let pool = Pool::builder(config).build().unwrap();

        // checkout a connection from the pool
        let mut conn = pool.get().await.unwrap();

        let u = User::select_by_user_id(123, &mut conn).await.unwrap();
        let r = RefreshTokens::select_join_user_by_token("123", &mut conn)
            .await
            .unwrap();
        dbg!(r);
    });
}
