use crate::DieselConn;
use chrono::{DateTime, Utc};
use diesel::OptionalExtension;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use uuid::Uuid;

table! {
    users(user_id) {
        user_id -> BigSerial,
        username -> Text,
        account -> Text,
        pwd -> Text,
        role -> Text,
        avatar_asset_id -> Nullable<Text>
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
    pub avatar_asset_id: Option<String>,
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
        family_id -> Uuid,
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
    pub family_id: Uuid,
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
    pub family_id: Uuid,
    pub device_id: Option<&'a str>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,

    pub ip_address: Option<ipnet::IpNet>,
    pub user_agent: Option<&'a str>,
}

allow_tables_to_appear_in_same_query!(users, refresh_tokens);

impl User {
    #[tracing::instrument(
        name = "db.user.select_by_user_id",
        skip_all,
        fields(
            db.system = "postgresql",
            db.operation = "select",
            db.table = "users",
            user.id = user_id,
        )
    )]
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

    #[tracing::instrument(
        name = "db.user.select_by_account",
        skip(account, conn),
        fields(
            db.system = "postgresql",
            db.operation = "select",
            db.table = "users",
        )
    )]
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

    #[tracing::instrument(
        name = "db.user.insert",
        skip(user, conn),
        fields(
            db.system = "postgresql",
            db.operation = "insert",
            db.table = "users",
        )
    )]
    pub async fn insert(
        user: &NewUser<'_>,
        conn: &mut DieselConn,
    ) -> Result<usize, diesel::result::Error> {
        diesel::insert_into(users::table)
            .values(user)
            .execute(conn)
            .await
    }

    #[tracing::instrument(
        name = "db.user.avatar.update",
        skip(conn),
        fields(
            db.system = "postgresql",
            db.operation = "update",
            db.table = "users",
            user.id = user_id,
            media.asset_id = %avatar_asset_id,
        )
    )]
    pub async fn update_avatar_asset_id(
        user_id: i64,
        avatar_asset_id: &str,
        conn: &mut DieselConn,
    ) -> Result<Self, diesel::result::Error> {
        diesel::update(users::table.find(user_id))
            .set(users::avatar_asset_id.eq(avatar_asset_id))
            .returning(Self::as_returning())
            .get_result::<Self>(conn)
            .await
    }
}

impl RefreshTokens {
    #[tracing::instrument(
        name = "db.refresh_token.consume",
        skip(hash, conn),
        fields(
            db.system = "postgresql",
            db.operation = "update",
            db.table = "refresh_tokens",
        )
    )]
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

    #[tracing::instrument(
        name = "db.refresh_token.select_by_hash",
        skip(hash, conn),
        fields(
            db.system = "postgresql",
            db.operation = "select",
            db.table = "refresh_tokens",
        )
    )]
    pub async fn select_by_token_hash(
        hash: &str,
        conn: &mut DieselConn,
    ) -> Result<Option<Self>, diesel::result::Error> {
        refresh_tokens::table
            .filter(refresh_tokens::token_hash.eq(hash))
            .select(Self::as_select())
            .first::<Self>(conn)
            .await
            .optional()
    }

    #[tracing::instrument(
        name = "db.refresh_token.family.revoke",
        skip(conn),
        fields(
            db.system = "postgresql",
            db.operation = "update",
            db.table = "refresh_tokens",
            auth.refresh_family_id = %family_id,
        )
    )]
    pub async fn revoke_active_family(
        family_id: Uuid,
        conn: &mut DieselConn,
    ) -> Result<usize, diesel::result::Error> {
        diesel::update(
            refresh_tokens::table
                .filter(refresh_tokens::family_id.eq(family_id))
                .filter(refresh_tokens::revoked_at.is_null()),
        )
        .set(refresh_tokens::revoked_at.eq(Utc::now()))
        .execute(conn)
        .await
    }

    #[tracing::instrument(
        name = "db.refresh_token.insert",
        skip(item, conn),
        fields(
            db.system = "postgresql",
            db.operation = "insert",
            db.table = "refresh_tokens",
            user.id = item.user_id,
        )
    )]
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
