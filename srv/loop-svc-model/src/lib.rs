pub mod account;

use deadpool::managed::Object;
use diesel_async::{AsyncPgConnection, pooled_connection::AsyncDieselConnectionManager};

pub type DieselConn = Object<AsyncDieselConnectionManager<AsyncPgConnection>>;

#[test]
fn f() {
    use crate::account::{RefreshTokens, User};
    use deadpool::managed::Pool;
    use diesel_async::pooled_connection::AsyncDieselConnectionManager;

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

        let _u = User::select_by_user_id(123, &mut conn).await.unwrap();
        let r = RefreshTokens::select_join_user_by_token("123", &mut conn)
            .await
            .unwrap();
        dbg!(r);
    });
}
