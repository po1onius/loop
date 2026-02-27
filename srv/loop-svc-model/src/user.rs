use crate::DieselConn;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use uuid::Uuid;

table! {
    users(user_id) {
        user_id -> Uuid,
        username -> VarChar,
        account -> VarChar,
        pwd -> VarChar
    }
}

#[derive(Queryable, Debug)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = users)]
pub struct User {
    pub user_id: Uuid,
    pub username: String,
    pub account: String,
    pub pwd: String,
}

#[derive(Insertable)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = users)]
pub struct NewUser<'a> {
    pub user_id: &'a Uuid,
    pub username: &'a str,
    pub account: &'a str,
    pub pwd: &'a str,
}

table! {
    admin(user_id) {
        user_id -> Uuid,
        account -> VarChar,
        pwd -> VarChar
    }
}

#[derive(Queryable, Debug)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = admin)]
pub struct Admin {
    pub user_id: Uuid,
    pub account: String,
    pub pwd: String,
}

impl User {
    pub async fn select_by_user_id(
        user_id: &Uuid,
        conn: &mut DieselConn,
    ) -> Result<Self, diesel::result::Error> {
        users::table.find(user_id).first::<User>(conn).await
    }

    pub async fn select_by_account(
        account: &str,
        conn: &mut DieselConn,
    ) -> Result<Option<Self>, diesel::result::Error> {
        users::table
            .filter(users::account.eq(account))
            .load::<User>(conn)
            .await
            .map(|mut v| v.pop())
    }

    pub async fn insert(
        user: &NewUser<'_>,
        conn: &mut DieselConn,
    ) -> Result<usize, diesel::result::Error> {
        diesel::insert_into(users::table)
            .values(user)
            .execute(conn)
            .await
    }
}

impl Admin {
    pub async fn select_by_account(
        account: &str,
        conn: &mut DieselConn,
    ) -> Result<Option<Self>, diesel::result::Error> {
        admin::table
            .filter(admin::account.eq(account))
            .load::<Admin>(conn)
            .await
            .map(|mut v| v.pop())
    }
}
