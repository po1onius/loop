use crate::{
    DieselConn,
    messaging::{MessageContract, MessageEnvelope, event::EventSearchRefreshV1, service_source},
};
use chrono::{DateTime, Utc};
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use uuid::Uuid;

table! {
    async_outbox(outbox_id) {
        outbox_id -> Uuid,
        topic -> Text,
        aggregate_id -> Text,
        payload -> Jsonb,
        created_at -> Timestamptz,
        published_at -> Nullable<Timestamptz>,
    }
}

#[derive(Queryable, Selectable, Debug)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = async_outbox)]
pub struct AsyncOutbox {
    pub outbox_id: Uuid,
    pub topic: String,
    pub aggregate_id: String,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Insertable, Debug)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[diesel(table_name = async_outbox)]
struct NewAsyncOutbox {
    outbox_id: Uuid,
    topic: String,
    aggregate_id: String,
    payload: serde_json::Value,
}

impl AsyncOutbox {
    #[tracing::instrument(
        name = "db.async_outbox.insert_event_search_refresh",
        skip(conn),
        fields(
            db.system = "postgresql",
            db.operation = "insert",
            db.table = "async_outbox",
            event.id = event_id,
            messaging.destination = EventSearchRefreshV1::ROUTING_KEY,
        )
    )]
    pub async fn insert_event_search_refresh(
        event_id: i64,
        producer_service: &str,
        conn: &mut DieselConn,
    ) -> Result<Self, diesel::result::Error> {
        // envelope.id 与 outbox 主键、AMQP message_id 使用同一个 UUID，日志、
        // 死信和重放工具可以跨 PostgreSQL 与 RabbitMQ 精确关联同一条消息。
        let outbox_id = Uuid::now_v7();
        let message = MessageEnvelope::new(
            outbox_id,
            service_source(producer_service),
            EventSearchRefreshV1 { event_id },
        );
        let payload = serde_json::to_value(message)
            .map_err(|error| diesel::result::Error::SerializationError(Box::new(error)))?;
        let row = NewAsyncOutbox {
            outbox_id,
            topic: EventSearchRefreshV1::ROUTING_KEY.to_string(),
            aggregate_id: event_id.to_string(),
            payload,
        };
        diesel::insert_into(async_outbox::table)
            .values(&row)
            .returning(Self::as_returning())
            .get_result(conn)
            .await
    }

    #[tracing::instrument(
        name = "db.async_outbox.select_pending_for_update",
        skip(conn),
        fields(
            db.system = "postgresql",
            db.operation = "select_for_update_skip_locked",
            db.table = "async_outbox",
            outbox.limit = limit,
        )
    )]
    pub async fn select_pending_for_update(
        limit: i64,
        conn: &mut DieselConn,
    ) -> Result<Vec<Self>, diesel::result::Error> {
        async_outbox::table
            .filter(async_outbox::published_at.is_null())
            .order((
                async_outbox::created_at.asc(),
                async_outbox::outbox_id.asc(),
            ))
            .limit(limit)
            .for_update()
            .skip_locked()
            .select(Self::as_select())
            .load(conn)
            .await
    }

    #[tracing::instrument(
        name = "db.async_outbox.mark_published",
        skip(conn),
        fields(
            db.system = "postgresql",
            db.operation = "update",
            db.table = "async_outbox",
            outbox.id = %outbox_id,
        )
    )]
    pub async fn mark_published(
        outbox_id: Uuid,
        published_at: DateTime<Utc>,
        conn: &mut DieselConn,
    ) -> Result<usize, diesel::result::Error> {
        diesel::update(
            async_outbox::table
                .filter(async_outbox::outbox_id.eq(outbox_id))
                .filter(async_outbox::published_at.is_null()),
        )
        .set(async_outbox::published_at.eq(Some(published_at)))
        .execute(conn)
        .await
    }

    #[tracing::instrument(
        name = "db.async_outbox.delete_published_before",
        skip(conn),
        fields(
            db.system = "postgresql",
            db.operation = "delete",
            db.table = "async_outbox",
            outbox.published_before = %published_before,
        )
    )]
    pub async fn delete_published_before(
        published_before: DateTime<Utc>,
        conn: &mut DieselConn,
    ) -> Result<usize, diesel::result::Error> {
        diesel::delete(async_outbox::table.filter(async_outbox::published_at.lt(published_before)))
            .execute(conn)
            .await
    }
}
