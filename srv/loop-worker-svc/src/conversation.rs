use anyhow::{Context, bail};
use diesel_async::AsyncConnection;
use futures_util::StreamExt;
use lapin::{
    Channel, Connection,
    options::*,
    types::{AMQPValue, FieldTable, LongString},
};
use loop_svc_model::{
    conversation::{Conversation, ConversationMessage},
    messaging::{MessageContract, MessageEnvelope, conversation::ConversationEvent},
    push,
};
use redis::AsyncCommands;

pub async fn declare(connection: &Connection, push: bool) -> anyhow::Result<Channel> {
    let channel = connection.create_channel().await?;
    let queue = queue_name(push);
    let dead = format!("{queue}.dead");
    let routing = if push {
        "conversation.push.dead"
    } else {
        "conversation.realtime.dead"
    };
    channel
        .queue_declare(
            dead.clone().into(),
            QueueDeclareOptions::durable(),
            FieldTable::default(),
        )
        .await?;
    channel
        .queue_bind(
            dead.into(),
            super::EVENT_DEAD_LETTER_EXCHANGE.into(),
            routing.into(),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await?;
    let mut arguments = FieldTable::default();
    arguments.insert(
        "x-dead-letter-exchange".into(),
        AMQPValue::LongString(LongString::from(super::EVENT_DEAD_LETTER_EXCHANGE)),
    );
    arguments.insert(
        "x-dead-letter-routing-key".into(),
        AMQPValue::LongString(LongString::from(routing)),
    );
    channel
        .queue_declare(queue.into(), QueueDeclareOptions::durable(), arguments)
        .await?;
    channel
        .queue_bind(
            queue.into(),
            super::EVENT_EXCHANGE.into(),
            ConversationEvent::ROUTING_KEY.into(),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await?;
    channel.basic_qos(16, BasicQosOptions::default()).await?;
    Ok(channel)
}

fn queue_name(push: bool) -> &'static str {
    if push {
        "loop.conversation.push.v1"
    } else {
        "loop.conversation.realtime.v1"
    }
}

pub async fn consume(channel: Channel, push: bool) -> anyhow::Result<()> {
    let mut consumer = channel
        .basic_consume(
            queue_name(push).into(),
            "".into(),
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await?;
    tracing::info!(
        event = "conversation.consumer.started",
        queue = queue_name(push),
        "conversation consumer started"
    );
    while let Some(delivery) = consumer.next().await {
        let delivery = delivery?;
        let message = serde_json::from_slice::<MessageEnvelope<ConversationEvent>>(&delivery.data);
        let message = match message {
            Ok(message)
                if message.validate_contract().is_ok()
                    && delivery
                        .properties
                        .content_type()
                        .as_ref()
                        .is_some_and(|v| {
                            v.as_str() == loop_svc_model::messaging::CLOUD_EVENTS_JSON_CONTENT_TYPE
                        })
                    && delivery
                        .properties
                        .kind()
                        .as_ref()
                        .is_some_and(|v| v.as_str() == ConversationEvent::MESSAGE_TYPE)
                    && delivery
                        .properties
                        .message_id()
                        .as_ref()
                        .is_some_and(|id| id.as_str() == message.id.to_string()) =>
            {
                message
            }
            _ => {
                tracing::error!(
                    event = "conversation.event.invalid",
                    queue = queue_name(push),
                    "invalid event sent to dead letter queue"
                );
                delivery
                    .nack(BasicNackOptions {
                        requeue: false,
                        ..Default::default()
                    })
                    .await?;
                continue;
            }
        };
        if let Err(error) = apply(&message.data, push).await {
            delivery
                .nack(BasicNackOptions {
                    requeue: true,
                    ..Default::default()
                })
                .await?;
            return Err(error).context("conversation consumer failed");
        }
        delivery.ack(BasicAckOptions::default()).await?;
        tracing::info!(event="conversation.event.processed", event_id=%message.id, conversation_id=%message.data.conversation_id, queue=queue_name(push), "conversation event processed");
    }
    bail!("conversation consumer ended")
}

async fn apply(event: &ConversationEvent, is_push: bool) -> anyhow::Result<()> {
    let mut conn = loop_infra::db::pg_pool()?.get().await?;
    let Some(conversation) = Conversation::select(event.conversation_id, &mut conn).await? else {
        return Ok(());
    };
    if conversation.status == "hidden" {
        return Ok(());
    }
    let message = if let Some(id) = event.message_id {
        ConversationMessage::select(id, &mut conn).await?
    } else {
        None
    };
    if event.message_id.is_some() && message.is_none() {
        return Ok(());
    }
    if message.as_ref().is_some_and(|m| {
        m.conversation_id != conversation.conversation_id || m.deleted_at.is_some()
    }) {
        return Ok(());
    }
    let recipients = if let Some(user_id) = event.user_id {
        vec![user_id]
    } else {
        conversation.recipients(&mut conn).await?
    };
    if is_push {
        if let Some(message) = message {
            if message.created_at <= chrono::Utc::now() - chrono::Duration::hours(24) {
                return Ok(());
            }
            // Delayed/replayed events do not notify members about messages from before they joined.
            conn.transaction::<(), anyhow::Error, _>(async |conn| {
                for user_id in recipients {
                    if user_id != message.sender_id
                        && conversation.can_read(user_id, conn).await?
                        && push::eligible(conversation.conversation_id, user_id, message.seq, conn)
                            .await?
                    {
                        push::enqueue(message.message_id, user_id, conn).await?;
                    }
                }
                Ok(())
            })
            .await?;
        }
    } else {
        let payload = if let Some(message) = message {
            serde_json::json!({"type":"conversation.message_created", "conversation_id":conversation.conversation_id, "message_id":message.message_id, "seq":message.seq.to_string()})
        } else { serde_json::json!({"type":"conversation.changed", "conversation_id":conversation.conversation_id}) }.to_string();
        let mut redis = loop_infra::redis::redis_pool()?.get().await?;
        if event.message_id.is_some() {
            redis
                .publish::<_, _, i64>(
                    format!("loop:conversation:{}", conversation.conversation_id),
                    &payload,
                )
                .await?;
        }
        for user_id in recipients {
            if conversation.can_read(user_id, &mut conn).await? {
                redis
                    .publish::<_, _, i64>(format!("loop:user:{user_id}"), &payload)
                    .await?;
            }
        }
    }
    Ok(())
}
