use crate::{DieselConn, account::users};
use chrono::{DateTime, Utc};
use diesel::{OptionalExtension, prelude::*};
use diesel_async::RunQueryDsl;
use uuid::Uuid;

table! {
    community_sections(section_id) {
        section_id -> Text,
        name -> Text,
        description -> Text,
        sort_order -> Int4,
        status -> Text,
        created_at -> Timestamptz,
    }
}

table! {
    conversations(conversation_id) {
        conversation_id -> Uuid,
        kind -> Text,
        subject_id -> Text,
        access_mode -> Text,
        status -> Text,
        title -> Text,
        last_seq -> Int8,
        message_count -> Int8,
        last_message_preview -> Nullable<Text>,
        last_message_at -> Nullable<Timestamptz>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

table! {
    community_posts(post_id) {
        post_id -> Uuid,
        author_id -> Int8,
        section_id -> Text,
        post_type -> Text,
        title -> Text,
        body -> Text,
        image_asset_ids -> Array<Text>,
        status -> Text,
        discussion_conversation_id -> Uuid,
        discussion_count -> Int8,
        interest_count -> Int8,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        last_activity_at -> Timestamptz,
        edited_at -> Nullable<Timestamptz>,
    }
}

table! {
    conversation_messages(message_id) {
        message_id -> Uuid,
        conversation_id -> Uuid,
        seq -> Int8,
        sender_id -> Int8,
        client_message_id -> Uuid,
        message_type -> Text,
        body -> Text,
        image_asset_ids -> Array<Text>,
        quote_message_id -> Nullable<Uuid>,
        created_at -> Timestamptz,
        edited_at -> Nullable<Timestamptz>,
        deleted_at -> Nullable<Timestamptz>,
    }
}

table! {
    conversation_read_states(conversation_id, user_id) {
        conversation_id -> Uuid,
        user_id -> Int8,
        last_read_seq -> Int8,
        subscribed -> Bool,
        muted -> Bool,
        updated_at -> Timestamptz,
    }
}

table! {
    community_post_reactions(post_id, user_id, reaction_type) {
        post_id -> Uuid,
        user_id -> Int8,
        reaction_type -> Text,
        created_at -> Timestamptz,
    }
}

table! {
    community_post_event_links(post_id, event_id, relation_type) {
        post_id -> Uuid,
        event_id -> Int8,
        relation_type -> Text,
        created_by -> Int8,
        created_at -> Timestamptz,
    }
}

#[derive(Clone, Debug, Queryable, Selectable)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = community_sections)]
pub struct CommunitySection {
    pub section_id: String,
    pub name: String,
    pub description: String,
    pub sort_order: i32,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Queryable, Selectable)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = conversations)]
pub struct Conversation {
    pub conversation_id: Uuid,
    pub kind: String,
    pub subject_id: String,
    pub access_mode: String,
    pub status: String,
    pub title: String,
    pub last_seq: i64,
    pub message_count: i64,
    pub last_message_preview: Option<String>,
    pub last_message_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = conversations)]
pub struct NewConversation<'a> {
    pub conversation_id: Uuid,
    pub kind: &'a str,
    pub subject_id: &'a str,
    pub access_mode: &'a str,
    pub status: &'a str,
    pub title: &'a str,
}

#[derive(Clone, Debug, Queryable, Selectable)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = community_posts)]
pub struct CommunityPost {
    pub post_id: Uuid,
    pub author_id: i64,
    pub section_id: String,
    pub post_type: String,
    pub title: String,
    pub body: String,
    pub image_asset_ids: Vec<String>,
    pub status: String,
    pub discussion_conversation_id: Uuid,
    pub discussion_count: i64,
    pub interest_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_activity_at: DateTime<Utc>,
    pub edited_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = community_posts)]
pub struct NewCommunityPost<'a> {
    pub post_id: Uuid,
    pub author_id: i64,
    pub section_id: &'a str,
    pub post_type: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    pub image_asset_ids: &'a [String],
    pub status: &'a str,
    pub discussion_conversation_id: Uuid,
}

#[derive(Clone, Debug)]
pub struct CommunityPostRecord {
    pub post: CommunityPost,
    pub author_username: String,
    pub author_avatar_asset_id: Option<String>,
    pub section_name: String,
}

#[derive(Debug, AsChangeset)]
#[diesel(table_name = community_posts)]
pub struct CommunityPostChanges<'a> {
    pub section_id: &'a str,
    pub post_type: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    pub image_asset_ids: &'a [String],
    pub updated_at: DateTime<Utc>,
    pub edited_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Queryable, Selectable)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = conversation_messages)]
pub struct ConversationMessage {
    pub message_id: Uuid,
    pub conversation_id: Uuid,
    pub seq: i64,
    pub sender_id: i64,
    pub client_message_id: Uuid,
    pub message_type: String,
    pub body: String,
    pub image_asset_ids: Vec<String>,
    pub quote_message_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub edited_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = conversation_messages)]
pub struct NewConversationMessage<'a> {
    pub message_id: Uuid,
    pub conversation_id: Uuid,
    pub seq: i64,
    pub sender_id: i64,
    pub client_message_id: Uuid,
    pub message_type: &'a str,
    pub body: &'a str,
    pub image_asset_ids: &'a [String],
    pub quote_message_id: Option<Uuid>,
}

#[derive(Clone, Debug)]
pub struct ConversationMessageRecord {
    pub message: ConversationMessage,
    pub sender_username: String,
    pub sender_avatar_asset_id: Option<String>,
}

#[derive(Clone, Debug, Queryable, Selectable)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = conversation_read_states)]
pub struct ConversationReadState {
    pub conversation_id: Uuid,
    pub user_id: i64,
    pub last_read_seq: i64,
    pub subscribed: bool,
    pub muted: bool,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = community_post_event_links)]
pub struct NewCommunityPostEventLink<'a> {
    pub post_id: Uuid,
    pub event_id: i64,
    pub relation_type: &'a str,
    pub created_by: i64,
}

#[derive(Clone, Debug)]
pub struct ConversationRecord {
    pub conversation: Conversation,
    pub read_state: Option<ConversationReadState>,
}

impl CommunitySection {
    #[tracing::instrument(name = "db.community_section.list", skip(conn))]
    pub async fn list_active(conn: &mut DieselConn) -> QueryResult<Vec<Self>> {
        community_sections::table
            .filter(community_sections::status.eq("active"))
            .order((
                community_sections::sort_order.asc(),
                community_sections::section_id.asc(),
            ))
            .select(Self::as_select())
            .load(conn)
            .await
    }

    pub async fn exists_active(section_id: &str, conn: &mut DieselConn) -> QueryResult<bool> {
        diesel::select(diesel::dsl::exists(
            community_sections::table
                .filter(community_sections::section_id.eq(section_id))
                .filter(community_sections::status.eq("active")),
        ))
        .get_result(conn)
        .await
    }
}

impl CommunityPost {
    pub async fn insert(item: NewCommunityPost<'_>, conn: &mut DieselConn) -> QueryResult<Self> {
        diesel::insert_into(community_posts::table)
            .values(item)
            .returning(Self::as_returning())
            .get_result(conn)
            .await
    }

    pub async fn select_record(
        post_id: Uuid,
        conn: &mut DieselConn,
    ) -> QueryResult<Option<CommunityPostRecord>> {
        let row = community_posts::table
            .inner_join(users::table.on(users::user_id.eq(community_posts::author_id)))
            .inner_join(
                community_sections::table
                    .on(community_sections::section_id.eq(community_posts::section_id)),
            )
            .filter(community_posts::post_id.eq(post_id))
            .filter(community_posts::status.eq("published"))
            .select((
                Self::as_select(),
                users::username,
                users::avatar_asset_id,
                community_sections::name,
            ))
            .first::<(Self, String, Option<String>, String)>(conn)
            .await
            .optional()?;
        Ok(row.map(
            |(post, author_username, author_avatar_asset_id, section_name)| CommunityPostRecord {
                post,
                author_username,
                author_avatar_asset_id,
                section_name,
            },
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn list_records(
        section_id: Option<&str>,
        author_id: Option<i64>,
        sort_by_activity: bool,
        cursor_at: Option<DateTime<Utc>>,
        cursor_id: Option<Uuid>,
        limit: i64,
        conn: &mut DieselConn,
    ) -> QueryResult<Vec<CommunityPostRecord>> {
        let mut query = community_posts::table
            .inner_join(users::table.on(users::user_id.eq(community_posts::author_id)))
            .inner_join(
                community_sections::table
                    .on(community_sections::section_id.eq(community_posts::section_id)),
            )
            .filter(community_posts::status.eq("published"))
            .into_boxed();
        if let Some(section_id) = section_id {
            query = query.filter(community_posts::section_id.eq(section_id));
        }
        if let Some(author_id) = author_id {
            query = query.filter(community_posts::author_id.eq(author_id));
        }
        if let (Some(cursor_at), Some(cursor_id)) = (cursor_at, cursor_id) {
            query = if sort_by_activity {
                query.filter(
                    community_posts::last_activity_at.lt(cursor_at).or(
                        community_posts::last_activity_at
                            .eq(cursor_at)
                            .and(community_posts::post_id.lt(cursor_id)),
                    ),
                )
            } else {
                query.filter(
                    community_posts::created_at
                        .lt(cursor_at)
                        .or(community_posts::created_at
                            .eq(cursor_at)
                            .and(community_posts::post_id.lt(cursor_id))),
                )
            };
        }
        query = if sort_by_activity {
            query.order((
                community_posts::last_activity_at.desc(),
                community_posts::post_id.desc(),
            ))
        } else {
            query.order((
                community_posts::created_at.desc(),
                community_posts::post_id.desc(),
            ))
        };
        let rows = query
            .select((
                Self::as_select(),
                users::username,
                users::avatar_asset_id,
                community_sections::name,
            ))
            .limit(limit)
            .load::<(Self, String, Option<String>, String)>(conn)
            .await?;
        Ok(rows
            .into_iter()
            .map(
                |(post, author_username, author_avatar_asset_id, section_name)| {
                    CommunityPostRecord {
                        post,
                        author_username,
                        author_avatar_asset_id,
                        section_name,
                    }
                },
            )
            .collect())
    }

    pub async fn update_owned(
        post_id: Uuid,
        author_id: i64,
        changes: CommunityPostChanges<'_>,
        conn: &mut DieselConn,
    ) -> QueryResult<Option<Self>> {
        diesel::update(
            community_posts::table
                .filter(community_posts::post_id.eq(post_id))
                .filter(community_posts::author_id.eq(author_id))
                .filter(community_posts::status.eq("published")),
        )
        .set(changes)
        .returning(Self::as_returning())
        .get_result(conn)
        .await
        .optional()
    }

    pub async fn soft_delete_owned(
        post_id: Uuid,
        author_id: i64,
        conn: &mut DieselConn,
    ) -> QueryResult<usize> {
        diesel::update(
            community_posts::table
                .filter(community_posts::post_id.eq(post_id))
                .filter(community_posts::author_id.eq(author_id))
                .filter(community_posts::status.eq("published")),
        )
        .set((
            community_posts::status.eq("deleted"),
            community_posts::updated_at.eq(Utc::now()),
        ))
        .execute(conn)
        .await
    }

    pub async fn update_discussion_stats(
        conversation_id: Uuid,
        last_activity_at: DateTime<Utc>,
        conn: &mut DieselConn,
    ) -> QueryResult<usize> {
        diesel::update(
            community_posts::table
                .filter(community_posts::discussion_conversation_id.eq(conversation_id)),
        )
        .set((
            community_posts::discussion_count.eq(community_posts::discussion_count + 1),
            community_posts::last_activity_at.eq(last_activity_at),
        ))
        .execute(conn)
        .await
    }
}

impl Conversation {
    pub async fn insert(item: NewConversation<'_>, conn: &mut DieselConn) -> QueryResult<Self> {
        diesel::insert_into(conversations::table)
            .values(item)
            .returning(Self::as_returning())
            .get_result(conn)
            .await
    }

    pub async fn select(conversation_id: Uuid, conn: &mut DieselConn) -> QueryResult<Option<Self>> {
        conversations::table
            .find(conversation_id)
            .select(Self::as_select())
            .first(conn)
            .await
            .optional()
    }

    pub async fn select_with_state(
        conversation_id: Uuid,
        user_id: i64,
        conn: &mut DieselConn,
    ) -> QueryResult<Option<ConversationRecord>> {
        let row = conversations::table
            .left_join(
                conversation_read_states::table.on(conversation_read_states::conversation_id
                    .eq(conversations::conversation_id)
                    .and(conversation_read_states::user_id.eq(user_id))),
            )
            .filter(conversations::conversation_id.eq(conversation_id))
            .select((
                Self::as_select(),
                Option::<ConversationReadState>::as_select(),
            ))
            .first::<(Self, Option<ConversationReadState>)>(conn)
            .await
            .optional()?;
        Ok(row.map(|(conversation, read_state)| ConversationRecord {
            conversation,
            read_state,
        }))
    }

    pub async fn list_subscribed(
        user_id: i64,
        conn: &mut DieselConn,
    ) -> QueryResult<Vec<ConversationRecord>> {
        let rows = conversations::table
            .inner_join(
                conversation_read_states::table.on(conversation_read_states::conversation_id
                    .eq(conversations::conversation_id)
                    .and(conversation_read_states::user_id.eq(user_id))),
            )
            .filter(conversation_read_states::subscribed.eq(true))
            .filter(conversations::status.ne("hidden"))
            .order((
                conversations::last_message_at.desc().nulls_last(),
                conversations::updated_at.desc(),
            ))
            .select((Self::as_select(), ConversationReadState::as_select()))
            .load::<(Self, ConversationReadState)>(conn)
            .await?;
        Ok(rows
            .into_iter()
            .map(|(conversation, read_state)| ConversationRecord {
                conversation,
                read_state: Some(read_state),
            })
            .collect())
    }

    pub async fn advance_for_message(
        conversation_id: Uuid,
        preview: &str,
        now: DateTime<Utc>,
        conn: &mut DieselConn,
    ) -> QueryResult<Option<Self>> {
        diesel::update(
            conversations::table
                .filter(conversations::conversation_id.eq(conversation_id))
                .filter(conversations::status.eq("active")),
        )
        .set((
            conversations::last_seq.eq(conversations::last_seq + 1),
            conversations::message_count.eq(conversations::message_count + 1),
            conversations::last_message_preview.eq(Some(preview)),
            conversations::last_message_at.eq(Some(now)),
            conversations::updated_at.eq(now),
        ))
        .returning(Self::as_returning())
        .get_result(conn)
        .await
        .optional()
    }

    pub async fn lock(conversation_id: Uuid, conn: &mut DieselConn) -> QueryResult<usize> {
        diesel::update(conversations::table.find(conversation_id))
            .set((
                conversations::status.eq("locked"),
                conversations::updated_at.eq(Utc::now()),
            ))
            .execute(conn)
            .await
    }

    pub async fn update_title(
        conversation_id: Uuid,
        title: &str,
        conn: &mut DieselConn,
    ) -> QueryResult<usize> {
        diesel::update(conversations::table.find(conversation_id))
            .set((
                conversations::title.eq(title),
                conversations::updated_at.eq(Utc::now()),
            ))
            .execute(conn)
            .await
    }
}

impl ConversationMessage {
    pub async fn insert(
        item: NewConversationMessage<'_>,
        conn: &mut DieselConn,
    ) -> QueryResult<Self> {
        diesel::insert_into(conversation_messages::table)
            .values(item)
            .returning(Self::as_returning())
            .get_result(conn)
            .await
    }

    pub async fn select_by_client_id(
        sender_id: i64,
        client_message_id: Uuid,
        conn: &mut DieselConn,
    ) -> QueryResult<Option<Self>> {
        conversation_messages::table
            .filter(conversation_messages::sender_id.eq(sender_id))
            .filter(conversation_messages::client_message_id.eq(client_message_id))
            .select(Self::as_select())
            .first(conn)
            .await
            .optional()
    }

    pub async fn select(message_id: Uuid, conn: &mut DieselConn) -> QueryResult<Option<Self>> {
        conversation_messages::table
            .find(message_id)
            .select(Self::as_select())
            .first(conn)
            .await
            .optional()
    }

    pub async fn select_record(
        message_id: Uuid,
        conn: &mut DieselConn,
    ) -> QueryResult<Option<ConversationMessageRecord>> {
        let row = conversation_messages::table
            .inner_join(users::table.on(users::user_id.eq(conversation_messages::sender_id)))
            .filter(conversation_messages::message_id.eq(message_id))
            .select((Self::as_select(), users::username, users::avatar_asset_id))
            .first::<(Self, String, Option<String>)>(conn)
            .await
            .optional()?;
        Ok(
            row.map(|(message, sender_username, sender_avatar_asset_id)| {
                ConversationMessageRecord {
                    message,
                    sender_username,
                    sender_avatar_asset_id,
                }
            }),
        )
    }

    pub async fn list_records(
        conversation_id: Uuid,
        before_seq: Option<i64>,
        limit: i64,
        conn: &mut DieselConn,
    ) -> QueryResult<Vec<ConversationMessageRecord>> {
        let mut query = conversation_messages::table
            .inner_join(users::table.on(users::user_id.eq(conversation_messages::sender_id)))
            .filter(conversation_messages::conversation_id.eq(conversation_id))
            .into_boxed();
        if let Some(before_seq) = before_seq {
            query = query.filter(conversation_messages::seq.lt(before_seq));
        }
        let rows = query
            .order(conversation_messages::seq.desc())
            .select((Self::as_select(), users::username, users::avatar_asset_id))
            .limit(limit)
            .load::<(Self, String, Option<String>)>(conn)
            .await?;
        Ok(rows
            .into_iter()
            .map(
                |(message, sender_username, sender_avatar_asset_id)| ConversationMessageRecord {
                    message,
                    sender_username,
                    sender_avatar_asset_id,
                },
            )
            .collect())
    }

    /// 从客户端已持有的最后序号向后补消息。实时通知只负责唤醒客户端，正文由
    /// 这里按升序分页读取，确保断线期间超过一页的消息也不会形成永久缺口。
    pub async fn list_records_after(
        conversation_id: Uuid,
        after_seq: i64,
        limit: i64,
        conn: &mut DieselConn,
    ) -> QueryResult<Vec<ConversationMessageRecord>> {
        let rows = conversation_messages::table
            .inner_join(users::table.on(users::user_id.eq(conversation_messages::sender_id)))
            .filter(conversation_messages::conversation_id.eq(conversation_id))
            .filter(conversation_messages::seq.gt(after_seq))
            .order(conversation_messages::seq.asc())
            .select((Self::as_select(), users::username, users::avatar_asset_id))
            .limit(limit)
            .load::<(Self, String, Option<String>)>(conn)
            .await?;
        Ok(rows
            .into_iter()
            .map(
                |(message, sender_username, sender_avatar_asset_id)| ConversationMessageRecord {
                    message,
                    sender_username,
                    sender_avatar_asset_id,
                },
            )
            .collect())
    }
}

impl ConversationReadState {
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert(
        conversation_id: Uuid,
        user_id: i64,
        last_read_seq: i64,
        subscribed: bool,
        muted: bool,
        conn: &mut DieselConn,
    ) -> QueryResult<Self> {
        diesel::insert_into(conversation_read_states::table)
            .values((
                conversation_read_states::conversation_id.eq(conversation_id),
                conversation_read_states::user_id.eq(user_id),
                conversation_read_states::last_read_seq.eq(last_read_seq),
                conversation_read_states::subscribed.eq(subscribed),
                conversation_read_states::muted.eq(muted),
                conversation_read_states::updated_at.eq(Utc::now()),
            ))
            .on_conflict((
                conversation_read_states::conversation_id,
                conversation_read_states::user_id,
            ))
            .do_update()
            .set((
                conversation_read_states::last_read_seq.eq(diesel::dsl::sql::<
                    diesel::sql_types::BigInt,
                >(
                    "GREATEST(conversation_read_states.last_read_seq, excluded.last_read_seq)",
                )),
                conversation_read_states::subscribed.eq(subscribed),
                conversation_read_states::muted.eq(muted),
                conversation_read_states::updated_at.eq(Utc::now()),
            ))
            .returning(Self::as_returning())
            .get_result(conn)
            .await
    }

    pub async fn mark_read(
        conversation_id: Uuid,
        user_id: i64,
        last_read_seq: i64,
        conn: &mut DieselConn,
    ) -> QueryResult<Self> {
        diesel::insert_into(conversation_read_states::table)
            .values((
                conversation_read_states::conversation_id.eq(conversation_id),
                conversation_read_states::user_id.eq(user_id),
                conversation_read_states::last_read_seq.eq(last_read_seq),
                conversation_read_states::subscribed.eq(false),
                conversation_read_states::muted.eq(false),
                conversation_read_states::updated_at.eq(Utc::now()),
            ))
            .on_conflict((
                conversation_read_states::conversation_id,
                conversation_read_states::user_id,
            ))
            .do_update()
            .set((
                conversation_read_states::last_read_seq.eq(diesel::dsl::sql::<
                    diesel::sql_types::BigInt,
                >(
                    "GREATEST(conversation_read_states.last_read_seq, excluded.last_read_seq)",
                )),
                conversation_read_states::updated_at.eq(Utc::now()),
            ))
            .returning(Self::as_returning())
            .get_result(conn)
            .await
    }

    pub async fn set_subscription(
        conversation_id: Uuid,
        user_id: i64,
        subscribed: bool,
        muted: bool,
        conn: &mut DieselConn,
    ) -> QueryResult<Self> {
        diesel::insert_into(conversation_read_states::table)
            .values((
                conversation_read_states::conversation_id.eq(conversation_id),
                conversation_read_states::user_id.eq(user_id),
                conversation_read_states::last_read_seq.eq(0_i64),
                conversation_read_states::subscribed.eq(subscribed),
                conversation_read_states::muted.eq(muted),
                conversation_read_states::updated_at.eq(Utc::now()),
            ))
            .on_conflict((
                conversation_read_states::conversation_id,
                conversation_read_states::user_id,
            ))
            .do_update()
            .set((
                conversation_read_states::subscribed.eq(subscribed),
                conversation_read_states::muted.eq(muted),
                conversation_read_states::updated_at.eq(Utc::now()),
            ))
            .returning(Self::as_returning())
            .get_result(conn)
            .await
    }
}

pub async fn select_interested_post_ids(
    user_id: i64,
    post_ids: &[Uuid],
    conn: &mut DieselConn,
) -> QueryResult<Vec<Uuid>> {
    community_post_reactions::table
        .filter(community_post_reactions::user_id.eq(user_id))
        .filter(community_post_reactions::reaction_type.eq("interested"))
        .filter(community_post_reactions::post_id.eq_any(post_ids))
        .select(community_post_reactions::post_id)
        .load(conn)
        .await
}

pub async fn add_interested_reaction(
    post_id: Uuid,
    user_id: i64,
    conn: &mut DieselConn,
) -> QueryResult<i64> {
    let inserted = diesel::insert_into(community_post_reactions::table)
        .values((
            community_post_reactions::post_id.eq(post_id),
            community_post_reactions::user_id.eq(user_id),
            community_post_reactions::reaction_type.eq("interested"),
        ))
        .on_conflict_do_nothing()
        .execute(conn)
        .await?;
    if inserted > 0 {
        diesel::update(
            community_posts::table
                .filter(community_posts::post_id.eq(post_id))
                .filter(community_posts::status.eq("published")),
        )
        .set(community_posts::interest_count.eq(community_posts::interest_count + 1))
        .returning(community_posts::interest_count)
        .get_result(conn)
        .await
    } else {
        community_posts::table
            .find(post_id)
            .select(community_posts::interest_count)
            .first(conn)
            .await
    }
}

pub async fn remove_interested_reaction(
    post_id: Uuid,
    user_id: i64,
    conn: &mut DieselConn,
) -> QueryResult<i64> {
    let removed = diesel::delete(
        community_post_reactions::table
            .filter(community_post_reactions::post_id.eq(post_id))
            .filter(community_post_reactions::user_id.eq(user_id))
            .filter(community_post_reactions::reaction_type.eq("interested")),
    )
    .execute(conn)
    .await?;
    if removed > 0 {
        diesel::update(community_posts::table.find(post_id))
            .set(
                community_posts::interest_count.eq(diesel::dsl::sql::<diesel::sql_types::BigInt>(
                    "GREATEST(interest_count - 1, 0)",
                )),
            )
            .returning(community_posts::interest_count)
            .get_result(conn)
            .await
    } else {
        community_posts::table
            .find(post_id)
            .select(community_posts::interest_count)
            .first(conn)
            .await
    }
}

pub async fn insert_post_event_link(
    item: NewCommunityPostEventLink<'_>,
    conn: &mut DieselConn,
) -> QueryResult<usize> {
    diesel::insert_into(community_post_event_links::table)
        .values(item)
        .on_conflict_do_nothing()
        .execute(conn)
        .await
}

pub async fn delete_post_event_links(
    post_id: Uuid,
    relation_type: &str,
    conn: &mut DieselConn,
) -> QueryResult<usize> {
    diesel::delete(
        community_post_event_links::table
            .filter(community_post_event_links::post_id.eq(post_id))
            .filter(community_post_event_links::relation_type.eq(relation_type)),
    )
    .execute(conn)
    .await
}

allow_tables_to_appear_in_same_query!(
    community_sections,
    conversations,
    community_posts,
    conversation_messages,
    conversation_read_states,
    community_post_reactions,
    community_post_event_links,
    users,
);
