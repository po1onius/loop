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
        content_version -> Int4,
        content_doc -> Jsonb,
        summary -> Text,
        cover_asset_id -> Nullable<Text>,
        start_at -> Nullable<Timestamptz>,
        end_at -> Nullable<Timestamptz>,
        location_name -> Nullable<Text>,
        location_address -> Nullable<Text>,
        capacity -> Nullable<Int4>,
        requires_approval -> Bool,
        tags -> Array<Text>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        published_at -> Nullable<Timestamptz>,
    }
}

table! {
    event_participations(event_id, user_id) {
        event_id -> Int8,
        user_id -> Int8,
        status -> Text,
        requested_at -> Timestamptz,
        reviewed_at -> Nullable<Timestamptz>,
        reviewed_by -> Nullable<Int8>,
        joined_at -> Nullable<Timestamptz>,
    }
}

// 参与申请列表需要展示申请人的用户名。这里仅声明查询所需的用户表字段，
// 不建立 Diesel association，也不依赖数据库外键。
table! {
    users(user_id) {
        user_id -> BigSerial,
        username -> Text,
        account -> Text,
        pwd -> Text,
        role -> Text,
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
    pub content_version: i32,
    pub content_doc: serde_json::Value,
    pub summary: String,
    pub cover_asset_id: Option<String>,
    pub start_at: Option<DateTime<Utc>>,
    pub end_at: Option<DateTime<Utc>>,
    pub location_name: Option<String>,
    pub location_address: Option<String>,
    pub capacity: Option<i32>,
    pub requires_approval: bool,
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
    pub content_version: i32,
    pub content_doc: serde_json::Value,
    pub summary: String,
    pub cover_asset_id: Option<String>,
    pub start_at: Option<DateTime<Utc>>,
    pub end_at: Option<DateTime<Utc>>,
    pub location_name: Option<String>,
    pub location_address: Option<String>,
    pub capacity: Option<i32>,
    pub requires_approval: bool,
    pub tags: Vec<String>,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(AsChangeset, Debug)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = events)]
#[diesel(treat_none_as_null = true)]
pub struct EventDraftChanges {
    pub title: String,
    pub content_version: i32,
    pub content_doc: serde_json::Value,
    pub summary: String,
    pub cover_asset_id: Option<String>,
    pub start_at: Option<DateTime<Utc>>,
    pub end_at: Option<DateTime<Utc>>,
    pub location_name: Option<String>,
    pub location_address: Option<String>,
    pub capacity: Option<i32>,
    pub requires_approval: bool,
    pub tags: Vec<String>,
}

#[derive(AsChangeset, Debug)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = events)]
#[diesel(treat_none_as_null = true)]
pub struct PublishEventDraftChanges {
    pub title: String,
    pub content_version: i32,
    pub content_doc: serde_json::Value,
    pub summary: String,
    pub cover_asset_id: Option<String>,
    pub start_at: Option<DateTime<Utc>>,
    pub end_at: Option<DateTime<Utc>>,
    pub location_name: Option<String>,
    pub location_address: Option<String>,
    pub capacity: Option<i32>,
    pub requires_approval: bool,
    pub tags: Vec<String>,
    pub status: String,
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

#[derive(Queryable, Selectable, Debug)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = event_participations)]
#[diesel(primary_key(event_id, user_id))]
pub struct EventParticipation {
    pub event_id: i64,
    pub user_id: i64,
    pub status: String,
    pub requested_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub reviewed_by: Option<i64>,
    pub joined_at: Option<DateTime<Utc>>,
}

#[derive(Insertable, Debug)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = event_participations)]
pub struct NewEventParticipation {
    pub event_id: i64,
    pub user_id: i64,
    pub status: String,
    pub requested_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub reviewed_by: Option<i64>,
    pub joined_at: Option<DateTime<Utc>>,
}

#[derive(Debug)]
pub struct EventParticipationApplicant {
    pub participation: EventParticipation,
    pub username: String,
}

allow_tables_to_appear_in_same_query!(events, event_participations, media_assets, users);

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

    #[tracing::instrument(
        name = "db.event.select_owned",
        skip(conn),
        fields(
            db.system = "postgresql",
            db.operation = "select",
            db.table = "events",
            user.id = owner_id,
            event.limit = limit,
            event.offset = offset,
        )
    )]
    pub async fn select_owned(
        owner_id: i64,
        status: Option<&str>,
        limit: i64,
        offset: i64,
        conn: &mut DieselConn,
    ) -> Result<Vec<Self>, diesel::result::Error> {
        let mut query = events::table
            .filter(events::creator_id.eq(owner_id))
            .into_boxed();
        if let Some(status) = status {
            query = query.filter(events::status.eq(status));
        }
        query
            .order(events::updated_at.desc())
            .limit(limit)
            .offset(offset)
            .select(Self::as_select())
            .load::<Self>(conn)
            .await
    }

    #[tracing::instrument(
        name = "db.event.select_joined_by_user",
        skip(conn),
        fields(
            db.system = "postgresql",
            db.operation = "select",
            db.table = "events,event_participations",
            user.id = user_id,
            event.limit = limit,
            event.offset = offset,
        )
    )]
    pub async fn select_joined_by_user(
        user_id: i64,
        limit: i64,
        offset: i64,
        conn: &mut DieselConn,
    ) -> Result<Vec<Self>, diesel::result::Error> {
        // “我参加的”只包含已经正式加入且活动仍处于发布状态的记录；
        // pending/rejected 参与关系不会出现在该列表中。
        event_participations::table
            .inner_join(events::table.on(events::event_id.eq(event_participations::event_id)))
            .filter(event_participations::user_id.eq(user_id))
            .filter(event_participations::status.eq("joined"))
            .filter(events::status.eq("published"))
            .order(event_participations::joined_at.desc())
            .limit(limit)
            .offset(offset)
            .select(Self::as_select())
            .load::<Self>(conn)
            .await
    }

    #[tracing::instrument(
        name = "db.event.select_published_by_id",
        skip(conn),
        fields(
            db.system = "postgresql",
            db.operation = "select",
            db.table = "events",
            event.id = event_id,
        )
    )]
    pub async fn select_published_by_id(
        event_id: i64,
        conn: &mut DieselConn,
    ) -> Result<Option<Self>, diesel::result::Error> {
        // 详情页只允许读取已发布活动，避免草稿或已取消活动通过 ID 被公开访问。
        events::table
            .filter(events::event_id.eq(event_id))
            .filter(events::status.eq("published"))
            .select(Self::as_select())
            .first::<Self>(conn)
            .await
            .optional()
    }

    #[tracing::instrument(
        name = "db.event.select_published_by_id_for_update",
        skip(conn),
        fields(
            db.system = "postgresql",
            db.operation = "select_for_update",
            db.table = "events",
            event.id = event_id,
        )
    )]
    pub async fn select_published_by_id_for_update(
        event_id: i64,
        conn: &mut DieselConn,
    ) -> Result<Option<Self>, diesel::result::Error> {
        // 所有改变参与人数的操作都先锁活动行，使容量检查与写入在同一活动内串行执行。
        events::table
            .filter(events::event_id.eq(event_id))
            .filter(events::status.eq("published"))
            .select(Self::as_select())
            .for_update()
            .first::<Self>(conn)
            .await
            .optional()
    }

    #[tracing::instrument(
        name = "db.event.select_owned_by_id",
        skip(conn),
        fields(
            db.system = "postgresql",
            db.operation = "select",
            db.table = "events",
            event.id = event_id,
            user.id = owner_id,
        )
    )]
    pub async fn select_owned_by_id(
        event_id: i64,
        owner_id: i64,
        conn: &mut DieselConn,
    ) -> Result<Option<Self>, diesel::result::Error> {
        events::table
            .filter(events::event_id.eq(event_id))
            .filter(events::creator_id.eq(owner_id))
            .select(Self::as_select())
            .first::<Self>(conn)
            .await
            .optional()
    }

    #[tracing::instrument(
        name = "db.event.select_owned_draft_for_update",
        skip(conn),
        fields(
            db.system = "postgresql",
            db.operation = "select_for_update",
            db.table = "events",
            event.id = event_id,
            user.id = owner_id,
        )
    )]
    pub async fn select_owned_draft_for_update(
        event_id: i64,
        owner_id: i64,
        conn: &mut DieselConn,
    ) -> Result<Option<Self>, diesel::result::Error> {
        // 发布草稿时先锁定目标行，避免自动保存或重复发布在校验和发布之间插入竞态写入。
        events::table
            .filter(events::event_id.eq(event_id))
            .filter(events::creator_id.eq(owner_id))
            .filter(events::status.eq("draft"))
            .select(Self::as_select())
            .for_update()
            .first::<Self>(conn)
            .await
            .optional()
    }

    #[tracing::instrument(
        name = "db.event.update_draft",
        skip(changes, conn),
        fields(
            db.system = "postgresql",
            db.operation = "update",
            db.table = "events",
            event.id = event_id,
            user.id = owner_id,
        )
    )]
    pub async fn update_draft(
        event_id: i64,
        owner_id: i64,
        changes: EventDraftChanges,
        conn: &mut DieselConn,
    ) -> Result<Self, diesel::result::Error> {
        diesel::update(
            events::table
                .filter(events::event_id.eq(event_id))
                .filter(events::creator_id.eq(owner_id))
                .filter(events::status.eq("draft")),
        )
        .set(changes)
        .returning(Self::as_returning())
        .get_result::<Self>(conn)
        .await
    }

    #[tracing::instrument(
        name = "db.event.publish_draft_with_changes",
        skip(changes, conn),
        fields(
            db.system = "postgresql",
            db.operation = "update",
            db.table = "events",
            event.id = event_id,
            user.id = owner_id,
        )
    )]
    pub async fn publish_draft_with_changes(
        event_id: i64,
        owner_id: i64,
        changes: PublishEventDraftChanges,
        conn: &mut DieselConn,
    ) -> Result<Self, diesel::result::Error> {
        // 把发布时的规范化内容和状态切换放进同一条 UPDATE，保证返回的已发布活动就是刚校验过的快照。
        diesel::update(
            events::table
                .filter(events::event_id.eq(event_id))
                .filter(events::creator_id.eq(owner_id))
                .filter(events::status.eq("draft")),
        )
        .set(changes)
        .returning(Self::as_returning())
        .get_result::<Self>(conn)
        .await
    }

    #[tracing::instrument(
        name = "db.event.delete_draft",
        skip(conn),
        fields(
            db.system = "postgresql",
            db.operation = "delete",
            db.table = "events",
            event.id = event_id,
            user.id = owner_id,
        )
    )]
    pub async fn delete_draft(
        event_id: i64,
        owner_id: i64,
        conn: &mut DieselConn,
    ) -> Result<usize, diesel::result::Error> {
        diesel::delete(
            events::table
                .filter(events::event_id.eq(event_id))
                .filter(events::creator_id.eq(owner_id))
                .filter(events::status.eq("draft")),
        )
        .execute(conn)
        .await
    }
}

impl EventParticipation {
    #[tracing::instrument(
        name = "db.event_participation.select",
        skip(conn),
        fields(
            db.system = "postgresql",
            db.operation = "select",
            db.table = "event_participations",
            event.id = event_id,
            user.id = user_id,
        )
    )]
    pub async fn select(
        event_id: i64,
        user_id: i64,
        conn: &mut DieselConn,
    ) -> Result<Option<Self>, diesel::result::Error> {
        event_participations::table
            .find((event_id, user_id))
            .select(Self::as_select())
            .first::<Self>(conn)
            .await
            .optional()
    }

    #[tracing::instrument(
        name = "db.event_participation.count_joined",
        skip(conn),
        fields(
            db.system = "postgresql",
            db.operation = "count",
            db.table = "event_participations",
            event.id = event_id,
        )
    )]
    pub async fn count_joined(
        event_id: i64,
        conn: &mut DieselConn,
    ) -> Result<i64, diesel::result::Error> {
        event_participations::table
            .filter(event_participations::event_id.eq(event_id))
            .filter(event_participations::status.eq("joined"))
            .count()
            .get_result::<i64>(conn)
            .await
    }

    #[tracing::instrument(
        name = "db.event_participation.insert",
        skip(participation, conn),
        fields(
            db.system = "postgresql",
            db.operation = "insert",
            db.table = "event_participations",
            event.id = participation.event_id,
            user.id = participation.user_id,
            participation.status = %participation.status,
        )
    )]
    pub async fn insert(
        participation: NewEventParticipation,
        conn: &mut DieselConn,
    ) -> Result<Self, diesel::result::Error> {
        diesel::insert_into(event_participations::table)
            .values(&participation)
            .returning(Self::as_returning())
            .get_result::<Self>(conn)
            .await
    }

    #[tracing::instrument(
        name = "db.event_participation.reapply",
        skip(conn),
        fields(
            db.system = "postgresql",
            db.operation = "update",
            db.table = "event_participations",
            event.id = event_id,
            user.id = user_id,
            participation.status = status,
        )
    )]
    pub async fn reapply(
        event_id: i64,
        user_id: i64,
        status: &str,
        requested_at: DateTime<Utc>,
        joined_at: Option<DateTime<Utc>>,
        conn: &mut DieselConn,
    ) -> Result<Self, diesel::result::Error> {
        diesel::update(
            event_participations::table
                .find((event_id, user_id))
                .filter(event_participations::status.eq("rejected")),
        )
        .set((
            event_participations::status.eq(status),
            event_participations::requested_at.eq(requested_at),
            event_participations::reviewed_at.eq(None::<DateTime<Utc>>),
            event_participations::reviewed_by.eq(None::<i64>),
            event_participations::joined_at.eq(joined_at),
        ))
        .returning(Self::as_returning())
        .get_result::<Self>(conn)
        .await
    }

    #[tracing::instrument(
        name = "db.event_participation.list_pending_applicants",
        skip(conn),
        fields(
            db.system = "postgresql",
            db.operation = "select",
            db.table = "event_participations",
            event.id = event_id,
        )
    )]
    pub async fn list_pending_applicants(
        event_id: i64,
        conn: &mut DieselConn,
    ) -> Result<Vec<EventParticipationApplicant>, diesel::result::Error> {
        let rows = event_participations::table
            .inner_join(users::table.on(users::user_id.eq(event_participations::user_id)))
            .filter(event_participations::event_id.eq(event_id))
            .filter(event_participations::status.eq("pending"))
            .order(event_participations::requested_at.asc())
            .select((Self::as_select(), users::username))
            .load::<(Self, String)>(conn)
            .await?;
        Ok(rows
            .into_iter()
            .map(|(participation, username)| EventParticipationApplicant {
                participation,
                username,
            })
            .collect())
    }

    #[tracing::instrument(
        name = "db.event_participation.review_pending",
        skip(conn),
        fields(
            db.system = "postgresql",
            db.operation = "update",
            db.table = "event_participations",
            event.id = event_id,
            user.id = user_id,
            reviewer.id = reviewer_id,
            participation.status = status,
        )
    )]
    pub async fn review_pending(
        event_id: i64,
        user_id: i64,
        reviewer_id: i64,
        status: &str,
        reviewed_at: DateTime<Utc>,
        joined_at: Option<DateTime<Utc>>,
        conn: &mut DieselConn,
    ) -> Result<Self, diesel::result::Error> {
        diesel::update(
            event_participations::table
                .find((event_id, user_id))
                .filter(event_participations::status.eq("pending")),
        )
        .set((
            event_participations::status.eq(status),
            event_participations::reviewed_at.eq(Some(reviewed_at)),
            event_participations::reviewed_by.eq(Some(reviewer_id)),
            event_participations::joined_at.eq(joined_at),
        ))
        .returning(Self::as_returning())
        .get_result::<Self>(conn)
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
