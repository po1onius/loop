use crate::{DieselConn, account::users};
use chrono::{DateTime, Utc};
use diesel::{OptionalExtension, prelude::*};
use diesel_async::RunQueryDsl;
use uuid::Uuid;

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

#[derive(Clone, Debug)]
pub struct ConversationRecord {
    pub conversation: Conversation,
    pub read_state: Option<ConversationReadState>,
}

impl Conversation {
    pub async fn lock_for_send(id: Uuid, conn: &mut DieselConn) -> QueryResult<Self> {
        conversations::table
            .find(id)
            .for_update()
            .select(Self::as_select())
            .first(conn)
            .await
    }

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
            .filter(diesel::dsl::sql::<diesel::sql_types::Bool>(
                "((conversations.kind = 'post_thread' AND conversation_read_states.subscribed) OR (conversations.kind = 'event_group' AND EXISTS (SELECT 1 FROM events e WHERE e.event_id::text = conversations.subject_id AND e.status = 'published' AND (e.creator_id = conversation_read_states.user_id OR EXISTS (SELECT 1 FROM event_participations p WHERE p.event_id = e.event_id AND p.user_id = conversation_read_states.user_id AND p.status = 'joined')))))"
            ))
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

allow_tables_to_appear_in_same_query!(
    conversations,
    conversation_messages,
    conversation_read_states,
    users
);

impl Conversation {
    /// Activity membership is the authority; read-state subscription never grants access.
    pub async fn can_read(&self, user_id: i64, conn: &mut DieselConn) -> QueryResult<bool> {
        if self.status == "hidden" {
            return Ok(false);
        }
        if self.kind == "post_thread" {
            return Ok(self.access_mode == "open");
        }
        if self.kind != "event_group" || self.access_mode != "restricted" {
            return Ok(false);
        }
        let Ok(event_id) = self.subject_id.parse::<i64>() else {
            return Ok(false);
        };
        let Some(event) = crate::event::Event::select_by_id(event_id, conn).await? else {
            return Ok(false);
        };
        if event.status != "published" {
            return Ok(false);
        }
        if event.creator_id == user_id {
            return Ok(true);
        }
        Ok(
            crate::event::EventParticipation::select(event_id, user_id, conn)
                .await?
                .is_some_and(|p| p.status == "joined"),
        )
    }

    pub async fn for_event(event_id: i64, conn: &mut DieselConn) -> QueryResult<Self> {
        conversations::table
            .filter(conversations::kind.eq("event_group"))
            .filter(conversations::subject_id.eq(event_id.to_string()))
            .select(Self::as_select())
            .first(conn)
            .await
    }

    /// Lock the conversation before establishing the initial read cursor, serializing with sends.
    pub async fn join_event(
        event_id: i64,
        user_id: i64,
        conn: &mut DieselConn,
    ) -> QueryResult<Self> {
        let conversation = conversations::table
            .filter(conversations::kind.eq("event_group"))
            .filter(conversations::subject_id.eq(event_id.to_string()))
            .for_update()
            .select(Self::as_select())
            .first::<Self>(conn)
            .await?;
        diesel::insert_into(conversation_read_states::table)
            .values((
                conversation_read_states::conversation_id.eq(conversation.conversation_id),
                conversation_read_states::user_id.eq(user_id),
                conversation_read_states::last_read_seq.eq(conversation.last_seq),
                conversation_read_states::subscribed.eq(true),
            ))
            .on_conflict_do_nothing()
            .execute(conn)
            .await?;
        crate::outbox::AsyncOutbox::insert_conversation_event(
            crate::messaging::conversation::ConversationEvent::changed(
                conversation.conversation_id,
                user_id,
            ),
            conn,
        )
        .await?;
        tracing::info!(event = "conversation.member.joined", %event_id, user_id, conversation_id = %conversation.conversation_id, "activity member entered group");
        Ok(conversation)
    }

    pub async fn create_for_event(
        event: &crate::event::Event,
        conn: &mut DieselConn,
    ) -> QueryResult<Self> {
        Self::insert(
            NewConversation {
                conversation_id: Uuid::now_v7(),
                kind: "event_group",
                subject_id: &event.event_id.to_string(),
                access_mode: "restricted",
                status: "active",
                title: &event.title,
            },
            conn,
        )
        .await?;
        Self::join_event(event.event_id, event.creator_id, conn).await
    }

    pub async fn recipients(&self, conn: &mut DieselConn) -> QueryResult<Vec<i64>> {
        #[derive(QueryableByName)]
        struct Recipient {
            #[diesel(sql_type = diesel::sql_types::BigInt)]
            user_id: i64,
        }
        let rows = if self.kind == "event_group" {
            diesel::sql_query("SELECT e.creator_id AS user_id FROM events e WHERE e.event_id::text = $1 AND e.status = 'published' UNION SELECT p.user_id FROM event_participations p JOIN events e ON e.event_id=p.event_id WHERE e.event_id::text=$1 AND e.status='published' AND p.status='joined'")
                .bind::<diesel::sql_types::Text,_>(&self.subject_id).load::<Recipient>(conn).await?
        } else {
            diesel::sql_query("SELECT user_id FROM conversation_read_states WHERE conversation_id=$1 AND subscribed=true")
                .bind::<diesel::sql_types::Uuid,_>(self.conversation_id).load::<Recipient>(conn).await?
        };
        Ok(rows.into_iter().map(|row| row.user_id).collect())
    }
}
