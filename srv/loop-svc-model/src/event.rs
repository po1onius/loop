use crate::DieselConn;
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;

table! {
    media_assets(asset_id) {
        asset_id -> Text,
        owner_id -> Int8,
        storage_key -> Text,
        mime_type -> Text,
        byte_size -> Int8,
        width -> Nullable<Int4>,
        height -> Nullable<Int4>,
        status -> Text,
        variants_json -> Jsonb,
        created_at -> Timestamptz,
    }
}

table! {
    events(event_id) {
        event_id -> BigSerial,
        creator_id -> Int8,
        title -> Text,
        status -> Text,
        content_doc -> Jsonb,
        summary -> Text,
        cover_asset_id -> Nullable<Text>,
        start_at -> Nullable<Timestamptz>,
        end_at -> Nullable<Timestamptz>,
        location_name -> Nullable<Text>,
        location_address -> Nullable<Text>,
        capacity -> Nullable<Int4>,
        tags -> Array<Text>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        published_at -> Nullable<Timestamptz>,
    }
}

#[derive(Queryable, Selectable, Debug)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = events)]
pub struct Event {
    pub event_id: i64,
    pub creator_id: i64,
    pub title: String,
    pub status: String,
    pub content_doc: serde_json::Value,
    pub summary: String,
    pub cover_asset_id: Option<String>,
    pub start_at: Option<DateTime<Utc>>,
    pub end_at: Option<DateTime<Utc>>,
    pub location_name: Option<String>,
    pub location_address: Option<String>,
    pub capacity: Option<i32>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Insertable, Debug)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = events)]
pub struct NewEvent {
    pub creator_id: i64,
    pub title: String,
    pub status: String,
    pub content_doc: serde_json::Value,
    pub summary: String,
    pub cover_asset_id: Option<String>,
    pub start_at: Option<DateTime<Utc>>,
    pub end_at: Option<DateTime<Utc>>,
    pub location_name: Option<String>,
    pub location_address: Option<String>,
    pub capacity: Option<i32>,
    pub tags: Vec<String>,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Queryable, Selectable, Debug)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = media_assets)]
pub struct MediaAsset {
    pub asset_id: String,
    pub owner_id: i64,
    pub storage_key: String,
    pub mime_type: String,
    pub byte_size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub status: String,
    pub variants_json: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Insertable, Debug)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = media_assets)]
pub struct NewMediaAsset {
    pub asset_id: String,
    pub owner_id: i64,
    pub storage_key: String,
    pub mime_type: String,
    pub byte_size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub status: String,
    pub variants_json: serde_json::Value,
}

allow_tables_to_appear_in_same_query!(events, media_assets);

impl Event {
    #[tracing::instrument(
        name = "db.event.insert",
        skip(event, conn),
        fields(
            db.system = "postgresql",
            db.operation = "insert",
            db.table = "events",
            user.id = event.creator_id,
        )
    )]
    pub async fn insert(
        event: NewEvent,
        conn: &mut DieselConn,
    ) -> Result<Self, diesel::result::Error> {
        diesel::insert_into(events::table)
            .values(&event)
            .returning(Self::as_returning())
            .get_result::<Self>(conn)
            .await
    }

    #[tracing::instrument(
        name = "db.event.select_published",
        skip(conn),
        fields(
            db.system = "postgresql",
            db.operation = "select",
            db.table = "events",
            event.limit = limit,
            event.offset = offset,
        )
    )]
    pub async fn select_published(
        limit: i64,
        offset: i64,
        conn: &mut DieselConn,
    ) -> Result<Vec<Self>, diesel::result::Error> {
        events::table
            .filter(events::status.eq("published"))
            .order(events::created_at.desc())
            .limit(limit)
            .offset(offset)
            .select(Self::as_select())
            .load::<Self>(conn)
            .await
    }
}

impl MediaAsset {
    #[tracing::instrument(
        name = "db.media_asset.insert",
        skip(asset, conn),
        fields(
            db.system = "postgresql",
            db.operation = "insert",
            db.table = "media_assets",
            user.id = asset.owner_id,
        )
    )]
    pub async fn insert(
        asset: NewMediaAsset,
        conn: &mut DieselConn,
    ) -> Result<Self, diesel::result::Error> {
        diesel::insert_into(media_assets::table)
            .values(&asset)
            .returning(Self::as_returning())
            .get_result::<Self>(conn)
            .await
    }

    #[tracing::instrument(
        name = "db.media_asset.select_by_asset_id",
        skip(conn),
        fields(
            db.system = "postgresql",
            db.operation = "select",
            db.table = "media_assets",
            media.asset_id = %asset_id,
        )
    )]
    pub async fn select_by_asset_id(
        asset_id: &str,
        conn: &mut DieselConn,
    ) -> Result<Option<Self>, diesel::result::Error> {
        media_assets::table
            .filter(media_assets::asset_id.eq(asset_id))
            .select(Self::as_select())
            .first::<Self>(conn)
            .await
            .optional()
    }

    #[tracing::instrument(
        name = "db.media_asset.select_uploaded_by_ids_for_owner",
        skip(asset_ids, conn),
        fields(
            db.system = "postgresql",
            db.operation = "select",
            db.table = "media_assets",
            user.id = owner_id,
            media.asset_count = asset_ids.len(),
        )
    )]
    pub async fn select_uploaded_by_ids_for_owner(
        asset_ids: &[String],
        owner_id: i64,
        conn: &mut DieselConn,
    ) -> Result<Vec<Self>, diesel::result::Error> {
        media_assets::table
            .filter(media_assets::owner_id.eq(owner_id))
            .filter(media_assets::asset_id.eq_any(asset_ids))
            .filter(media_assets::status.eq("uploaded"))
            .select(Self::as_select())
            .load::<Self>(conn)
            .await
    }

    #[tracing::instrument(
        name = "db.media_asset.select_uploaded_by_asset_id",
        skip(conn),
        fields(
            db.system = "postgresql",
            db.operation = "select",
            db.table = "media_assets",
            media.asset_id = %asset_id,
        )
    )]
    pub async fn select_uploaded_by_asset_id(
        asset_id: &str,
        conn: &mut DieselConn,
    ) -> Result<Option<Self>, diesel::result::Error> {
        media_assets::table
            .filter(media_assets::asset_id.eq(asset_id))
            .filter(media_assets::status.eq("uploaded"))
            .select(Self::as_select())
            .first::<Self>(conn)
            .await
            .optional()
    }

    #[tracing::instrument(
        name = "db.media_asset.mark_uploaded",
        skip(conn),
        fields(
            db.system = "postgresql",
            db.operation = "update",
            db.table = "media_assets",
            user.id = owner_id,
            media.asset_id = %asset_id,
        )
    )]
    pub async fn mark_uploaded(
        asset_id: &str,
        owner_id: i64,
        conn: &mut DieselConn,
    ) -> Result<Self, diesel::result::Error> {
        diesel::update(
            media_assets::table
                .filter(media_assets::asset_id.eq(asset_id))
                .filter(media_assets::owner_id.eq(owner_id)),
        )
        .set(media_assets::status.eq("uploaded"))
        .returning(Self::as_returning())
        .get_result::<Self>(conn)
        .await
    }
}
