use crate::DieselConn;
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;

table! {
    users(user_id) {
        user_id -> BigSerial,
        username -> Text,
        account -> Text,
        pwd -> Text
    }
}

#[derive(Queryable, Debug)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = users)]
pub struct User {
    pub user_id: i64,
    pub username: String,
    pub account: String,
    pub pwd: String,
}

#[derive(Insertable)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = users)]
pub struct NewUser<'a> {
    pub username: &'a str,
    pub account: &'a str,
    pub pwd: &'a str,
}

/*
CREATE TABLE refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    device_id TEXT,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()

    ip_address INET,
    user_agent TEXT,
);
 */

table! {
    refresh_tokens(id) {
        id -> BigSerial,
        user_id -> BigSerial,
        token_hash -> Text,
        device_id -> Nullable<Text>,
        expires_at -> Timestamptz,
        revoked_at -> Nullable<Timestamptz>,
        created_at -> Timestamptz,

        ip_address -> Nullable<Inet>,
        user_agent -> Nullable<Text>
    }
}

#[derive(Queryable, Debug)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = refresh_tokens)]
pub struct RefreshTokens {
    pub id: i64,
    pub user_id: i64,
    pub token_hash: String,
    pub device_id: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,

    pub ip_address: Option<ipnet::IpNet>,
    pub user_agent: Option<String>,
}

#[derive(Insertable, Debug)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = refresh_tokens)]
pub struct NewRefreshTokens<'a> {
    pub user_id: i64,
    pub token_hash: &'a str,
    pub device_id: Option<&'a str>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,

    pub ip_address: Option<ipnet::IpNet>,
    pub user_agent: Option<&'a str>,
}

impl User {
    pub async fn select_by_user_id(
        user_id: i64,
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

impl RefreshTokens {
    pub async fn select_by_token_hash(
        hash: &str,
        conn: &mut DieselConn,
    ) -> Result<Option<Self>, diesel::result::Error> {
        refresh_tokens::table
            .filter(refresh_tokens::token_hash.eq(hash))
            .load::<Self>(conn)
            .await
            .map(|mut v| v.pop())
    }

    pub async fn expire(id: i64, conn: &mut DieselConn) -> Result<usize, diesel::result::Error> {
        diesel::update(refresh_tokens::dsl::refresh_tokens.find(id))
            .set(refresh_tokens::revoked_at.eq(Utc::now()))
            .execute(conn)
            .await
    }

    pub async fn insert(
        item: NewRefreshTokens<'_>,
        conn: &mut DieselConn,
    ) -> Result<usize, diesel::result::Error> {
        diesel::insert_into(refresh_tokens::table)
            .values(item)
            .execute(conn)
            .await
    }
}
