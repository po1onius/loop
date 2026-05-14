use crate::DieselConn;
use chrono::{DateTime, Utc};
use diesel::OptionalExtension;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;

table! {
    users(user_id) {
        user_id -> BigSerial,
        username -> Text,
        account -> Text,
        pwd -> Text,
        role -> Text
    }
}

#[derive(Queryable, Selectable, Debug)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = users)]
pub struct User {
    pub user_id: i64,
    pub username: String,
    pub account: String,
    pub pwd: String,
    pub role: String,
}

#[derive(Insertable)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = users)]
pub struct NewUser<'a> {
    pub username: &'a str,
    pub account: &'a str,
    pub pwd: &'a str,
    pub role: &'a str,
}

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

#[derive(Queryable, Selectable, Debug)]
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

joinable!(refresh_tokens -> users (user_id));
allow_tables_to_appear_in_same_query!(users, refresh_tokens);

impl User {
    pub async fn select_by_user_id(
        user_id: i64,
        conn: &mut DieselConn,
    ) -> Result<Option<Self>, diesel::result::Error> {
        users::table
            .find(user_id)
            .first::<User>(conn)
            .await
            .map_or_else(
                |e| {
                    if e == diesel::NotFound {
                        Ok(None)
                    } else {
                        Err(e)
                    }
                },
                |u| Ok(Some(u)),
            )
    }

    pub async fn select_by_account(
        account: &str,
        conn: &mut DieselConn,
    ) -> Result<Option<Self>, diesel::result::Error> {
        users::table
            .filter(users::account.eq(account))
            .first::<User>(conn)
            .await
            .optional()
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
    pub async fn consume_by_token_hash(
        hash: &str,
        conn: &mut DieselConn,
    ) -> Result<Option<Self>, diesel::result::Error> {
        diesel::update(
            refresh_tokens::table
                .filter(refresh_tokens::token_hash.eq(hash))
                .filter(refresh_tokens::revoked_at.is_null()),
        )
        .set(refresh_tokens::revoked_at.eq(Utc::now()))
        .returning(Self::as_returning())
        .get_result::<Self>(conn)
        .await
        .optional()
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
