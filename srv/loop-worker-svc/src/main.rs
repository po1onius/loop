use anyhow::{Context, bail};
use chrono::{Duration as ChronoDuration, Utc};
use diesel_async::AsyncConnection;
use futures_util::StreamExt;
use lapin::{
    BasicProperties, Channel, Connection, ExchangeKind,
    options::{
        BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicPublishOptions,
        BasicQosOptions, ConfirmSelectOptions, ExchangeDeclareOptions, QueueBindOptions,
        QueueDeclareOptions,
    },
    types::{AMQPValue, FieldTable, LongString},
};
use loop_infra::{
    config::InfraConfig,
    observability::{ObservabilityConfig, init as init_observability},
};
use loop_search::EventSearchService;
use loop_svc_model::{
    event::Event,
    messaging::{
        CLOUD_EVENTS_JSON_CONTENT_TYPE, MessageContract, MessageEnvelope,
        event::{EVENT_SEARCH_REFRESH_ROUTING_KEY, EventSearchRefreshV1},
    },
    outbox::AsyncOutbox,
};
use std::{process::ExitCode, time::Duration};

const EVENT_EXCHANGE: &str = "loop.events.v1";
const EVENT_DEAD_LETTER_EXCHANGE: &str = "loop.events.dlx.v1";
const EVENT_SEARCH_QUEUE: &str = "loop.search.events.v1";
const EVENT_SEARCH_DEAD_LETTER_QUEUE: &str = "loop.search.events.dead.v1";
const EVENT_SEARCH_CONSUMER_TAG: &str = "loop-worker-event-search-v1";
const OUTBOX_BATCH_SIZE: i64 = 100;
const OUTBOX_IDLE_INTERVAL: Duration = Duration::from_millis(500);
const OUTBOX_RETENTION_DAYS: i64 = 7;
const OUTBOX_CLEANUP_INTERVAL: Duration = Duration::from_secs(60 * 60);
const CONSUMER_PREFETCH: u16 = 16;

pub const SERVICE_NAME: &str = "loop-worker-svc";

#[tokio::main]
async fn main() -> ExitCode {
    let observability = init_observability(ObservabilityConfig::new(
        SERVICE_NAME,
        env!("CARGO_PKG_VERSION"),
    ));
    let _observability = match observability {
        Ok(guard) => guard,
        Err(error) => {
            log_bootstrap_error("failed to initialize worker observability", &error);
            return ExitCode::FAILURE;
        }
    };

    if let Err(error) = run().await {
        tracing::error!(
            event = "worker.exit",
            error_chain = %format_error_chain(&error),
            error_source = ?error,
            "worker exited with error"
        );
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

async fn run() -> anyhow::Result<()> {
    InfraConfig::from_env()?.init()?;

    let search = EventSearchService::from_env()?;
    // worker 启动时先同步存量活动，再开始消费增量消息。同步过程是幂等 upsert，
    // 多个 worker 同时启动不会清空共享索引或产生不可见窗口。
    search
        .initialize_writer_and_sync()
        .await
        .context("failed to initialize event search writer")?;

    let connection = loop_infra::rabbitmq::connect(SERVICE_NAME).await?;
    let topology = declare_topology(&connection).await?;
    tracing::info!(
        event = "worker.start",
        service_name = SERVICE_NAME,
        messaging_system = "rabbitmq",
        messaging_destination = EVENT_SEARCH_QUEUE,
        "worker started"
    );

    // 三个长期任务任意一个异常退出都终止进程，让 K8s/Compose 按统一方式重启，
    // 避免 worker 表面存活但已经停止投递或消费。
    tokio::try_join!(
        run_outbox_relay(topology.publisher),
        run_event_search_consumer(topology.consumer, search),
        run_outbox_cleanup(),
    )?;
    Ok(())
}

struct WorkerTopology {
    publisher: Channel,
    consumer: Channel,
}

async fn declare_topology(connection: &Connection) -> anyhow::Result<WorkerTopology> {
    let publisher = connection
        .create_channel()
        .await
        .context("failed to create RabbitMQ publisher channel")?;
    let consumer = connection
        .create_channel()
        .await
        .context("failed to create RabbitMQ consumer channel")?;

    publisher
        .exchange_declare(
            EVENT_EXCHANGE.into(),
            ExchangeKind::Topic,
            ExchangeDeclareOptions {
                durable: true,
                ..ExchangeDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .context("failed to declare event exchange")?;
    publisher
        .exchange_declare(
            EVENT_DEAD_LETTER_EXCHANGE.into(),
            ExchangeKind::Direct,
            ExchangeDeclareOptions {
                durable: true,
                ..ExchangeDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .context("failed to declare event dead-letter exchange")?;

    let mut queue_arguments = FieldTable::default();
    queue_arguments.insert(
        "x-dead-letter-exchange".into(),
        AMQPValue::LongString(LongString::from(EVENT_DEAD_LETTER_EXCHANGE)),
    );
    publisher
        .queue_declare(
            EVENT_SEARCH_QUEUE.into(),
            QueueDeclareOptions::durable(),
            queue_arguments,
        )
        .await
        .context("failed to declare event search queue")?;
    publisher
        .queue_bind(
            EVENT_SEARCH_QUEUE.into(),
            EVENT_EXCHANGE.into(),
            EVENT_SEARCH_REFRESH_ROUTING_KEY.into(),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .context("failed to bind event search queue")?;

    publisher
        .queue_declare(
            EVENT_SEARCH_DEAD_LETTER_QUEUE.into(),
            QueueDeclareOptions::durable(),
            FieldTable::default(),
        )
        .await
        .context("failed to declare event search dead-letter queue")?;
    publisher
        .queue_bind(
            EVENT_SEARCH_DEAD_LETTER_QUEUE.into(),
            EVENT_DEAD_LETTER_EXCHANGE.into(),
            EVENT_SEARCH_REFRESH_ROUTING_KEY.into(),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .context("failed to bind event search dead-letter queue")?;

    publisher
        .confirm_select(ConfirmSelectOptions::default())
        .await
        .context("failed to enable RabbitMQ publisher confirms")?;
    consumer
        .basic_qos(CONSUMER_PREFETCH, BasicQosOptions::default())
        .await
        .context("failed to configure RabbitMQ consumer prefetch")?;
    tracing::info!(
        event = "worker.rabbitmq.topology_declared",
        messaging_exchange = EVENT_EXCHANGE,
        messaging_queue = EVENT_SEARCH_QUEUE,
        dead_letter_queue = EVENT_SEARCH_DEAD_LETTER_QUEUE,
        consumer_prefetch = CONSUMER_PREFETCH,
        "RabbitMQ worker topology declared"
    );
    Ok(WorkerTopology {
        publisher,
        consumer,
    })
}

async fn run_outbox_relay(publisher: Channel) -> anyhow::Result<()> {
    loop {
        let published = relay_outbox_batch(&publisher).await?;
        if published == 0 {
            tokio::time::sleep(OUTBOX_IDLE_INTERVAL).await;
        }
    }
}

#[tracing::instrument(name = "worker.outbox.relay_batch", skip_all)]
async fn relay_outbox_batch(publisher: &Channel) -> anyhow::Result<usize> {
    let pool = loop_infra::db::pg_pool().context("failed to get PostgreSQL pool")?;
    let mut conn = pool
        .get()
        .await
        .context("failed to get PostgreSQL connection for outbox relay")?;
    conn.transaction::<usize, anyhow::Error, _>(async |conn| {
        let rows = AsyncOutbox::select_pending_for_update(OUTBOX_BATCH_SIZE, conn)
            .await
            .context("failed to lock pending outbox rows")?;
        let row_count = rows.len();
        for row in rows {
            publish_outbox_row(publisher, &row).await?;
            let updated = AsyncOutbox::mark_published(row.outbox_id, Utc::now(), conn)
                .await
                .context("failed to mark outbox row as published")?;
            if updated != 1 {
                bail!("outbox row {} was not marked as published", row.outbox_id);
            }
            tracing::info!(
                event = "worker.outbox.published",
                outbox_id = %row.outbox_id,
                messaging_destination = %row.topic,
                aggregate_id = %row.aggregate_id,
                outbox_created_at = %row.created_at,
                "outbox message published and confirmed"
            );
        }
        Ok(row_count)
    })
    .await
}

async fn publish_outbox_row(publisher: &Channel, row: &AsyncOutbox) -> anyhow::Result<()> {
    let payload = serde_json::to_vec(&row.payload).context("failed to encode outbox payload")?;
    let envelope_id = cloud_event_attribute(&row.payload, "id")?;
    if envelope_id != row.outbox_id.to_string() {
        bail!(
            "CloudEvent id {} does not match outbox id {}",
            envelope_id,
            row.outbox_id
        );
    }
    let spec_version = cloud_event_attribute(&row.payload, "specversion")?;
    if spec_version != "1.0" {
        bail!("unsupported CloudEvent specversion {spec_version}");
    }
    let message_type = cloud_event_attribute(&row.payload, "type")?;
    let correlation_id = row
        .payload
        .get("correlationid")
        .and_then(serde_json::Value::as_str);
    let timestamp = u64::try_from(row.created_at.timestamp())
        .context("outbox created_at is before the Unix epoch")?;
    let mut properties = BasicProperties::default()
        .with_content_type(CLOUD_EVENTS_JSON_CONTENT_TYPE.into())
        .with_delivery_mode(2)
        .with_message_id(row.outbox_id.to_string().into())
        .with_type(message_type.into())
        .with_timestamp(timestamp);
    if let Some(correlation_id) = correlation_id {
        properties = properties.with_correlation_id(correlation_id.into());
    }
    let confirm = publisher
        .basic_publish(
            EVENT_EXCHANGE.into(),
            row.topic.clone().into(),
            BasicPublishOptions {
                mandatory: true,
                ..BasicPublishOptions::default()
            },
            &payload,
            properties,
        )
        .await
        .context("failed to publish outbox message")?
        .await
        .context("failed to receive RabbitMQ publisher confirm")?;
    if !confirm.is_ack() {
        bail!(
            "RabbitMQ negatively acknowledged outbox row {}",
            row.outbox_id
        );
    }
    if confirm.take_message().is_some() {
        bail!("RabbitMQ returned unroutable outbox row {}", row.outbox_id);
    }
    Ok(())
}

fn cloud_event_attribute<'a>(
    payload: &'a serde_json::Value,
    attribute: &'static str,
) -> anyhow::Result<&'a str> {
    payload
        .get(attribute)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .with_context(|| format!("outbox payload is missing CloudEvent attribute {attribute}"))
}

async fn run_event_search_consumer(
    channel: Channel,
    search: EventSearchService,
) -> anyhow::Result<()> {
    let mut consumer = channel
        .basic_consume(
            EVENT_SEARCH_QUEUE.into(),
            EVENT_SEARCH_CONSUMER_TAG.into(),
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .context("failed to start event search consumer")?;
    tracing::info!(
        event = "worker.consumer.started",
        messaging_destination = EVENT_SEARCH_QUEUE,
        consumer_tag = EVENT_SEARCH_CONSUMER_TAG,
        "event search consumer started"
    );

    while let Some(delivery) = consumer.next().await {
        let delivery = delivery.context("RabbitMQ consumer stream returned an error")?;
        let message = match decode_event_search_message(&delivery) {
            Ok(message) => message,
            Err(error) => {
                tracing::error!(
                    event = "worker.message.invalid",
                    delivery_tag = delivery.delivery_tag,
                    error_chain = %format_error_chain(&error),
                    "invalid event search CloudEvent; rejecting to dead-letter queue"
                );
                delivery
                    .nack(BasicNackOptions {
                        requeue: false,
                        ..BasicNackOptions::default()
                    })
                    .await
                    .context("failed to reject invalid event search message")?;
                continue;
            }
        };

        if let Err(error) = apply_event_message(&search, &message).await {
            tracing::error!(
                event = "worker.message.processing_failed",
                message_id = %message.id,
                message_type = %message.ty,
                correlation_id = message.correlationid.as_deref(),
                event_id = message.data.event_id,
                delivery_tag = delivery.delivery_tag,
                redelivered = delivery.redelivered,
                error_chain = %format_error_chain(&error),
                "event search message processing failed; requeueing before worker exit"
            );
            delivery
                .nack(BasicNackOptions {
                    requeue: true,
                    ..BasicNackOptions::default()
                })
                .await
                .context("failed to requeue event search message")?;
            return Err(error);
        }
        delivery
            .ack(BasicAckOptions::default())
            .await
            .context("failed to acknowledge event search message")?;
        tracing::info!(
            event = "worker.message.processed",
            message_id = %message.id,
            message_type = %message.ty,
            message_source = %message.source,
            correlation_id = message.correlationid.as_deref(),
            event_id = message.data.event_id,
            delivery_tag = delivery.delivery_tag,
            redelivered = delivery.redelivered,
            "event search message processed and acknowledged"
        );
    }
    bail!("RabbitMQ event search consumer stream ended unexpectedly")
}

fn decode_event_search_message(
    delivery: &lapin::message::Delivery,
) -> anyhow::Result<MessageEnvelope<EventSearchRefreshV1>> {
    let message = serde_json::from_slice::<MessageEnvelope<EventSearchRefreshV1>>(&delivery.data)
        .context("failed to decode event search CloudEvent")?;
    message
        .validate_contract()
        .context("event search CloudEvent contract validation failed")?;

    // structured mode 的消息体可以独立解析，但同时校验 AMQP properties，尽早
    // 发现错误 publisher 或人工重放时的 metadata/body 不一致。
    let content_type = delivery
        .properties
        .content_type()
        .as_ref()
        .map(ToString::to_string)
        .context("AMQP content_type is required")?;
    if content_type != CLOUD_EVENTS_JSON_CONTENT_TYPE {
        bail!("unexpected AMQP content_type {content_type}");
    }
    let message_id = delivery
        .properties
        .message_id()
        .as_ref()
        .map(ToString::to_string)
        .context("AMQP message_id is required")?;
    if message_id != message.id.to_string() {
        bail!(
            "AMQP message_id {message_id} does not match CloudEvent id {}",
            message.id
        );
    }
    let message_type = delivery
        .properties
        .kind()
        .as_ref()
        .map(ToString::to_string)
        .context("AMQP type is required")?;
    if message_type != EventSearchRefreshV1::MESSAGE_TYPE {
        bail!("unexpected AMQP message type {message_type}");
    }
    let correlation_id = delivery
        .properties
        .correlation_id()
        .as_ref()
        .map(ToString::to_string);
    if correlation_id != message.correlationid {
        bail!("AMQP correlation_id does not match CloudEvent correlationid");
    }
    Ok(message)
}

#[tracing::instrument(
    name = "worker.event_search.apply_message",
    skip_all,
    fields(message.id = %message.id, event.id = message.data.event_id)
)]
async fn apply_event_message(
    search: &EventSearchService,
    message: &MessageEnvelope<EventSearchRefreshV1>,
) -> anyhow::Result<()> {
    let pool = loop_infra::db::pg_pool().context("failed to get PostgreSQL pool")?;
    let mut conn = pool
        .get()
        .await
        .context("failed to get PostgreSQL connection for event indexing")?;
    match Event::select_by_id(message.data.event_id, &mut conn)
        .await
        .context("failed to load event for search indexing")?
    {
        Some(event) => search.apply_event(&event).await,
        None => search.remove_event(message.data.event_id).await,
    }
}

async fn run_outbox_cleanup() -> anyhow::Result<()> {
    let mut interval = tokio::time::interval(OUTBOX_CLEANUP_INTERVAL);
    // interval 的第一次 tick 会立即完成；启动时顺便清理历史已发布记录。
    loop {
        interval.tick().await;
        let published_before = Utc::now() - ChronoDuration::days(OUTBOX_RETENTION_DAYS);
        let pool = loop_infra::db::pg_pool().context("failed to get PostgreSQL pool")?;
        let mut conn = pool
            .get()
            .await
            .context("failed to get PostgreSQL connection for outbox cleanup")?;
        let deleted = AsyncOutbox::delete_published_before(published_before, &mut conn)
            .await
            .context("failed to clean published outbox rows")?;
        tracing::info!(
            event = "worker.outbox.cleanup_completed",
            deleted_count = deleted,
            retention_days = OUTBOX_RETENTION_DAYS,
            "published outbox cleanup completed"
        );
    }
}

fn log_bootstrap_error(message: &str, error: &anyhow::Error) {
    let line = serde_json::json!({
        "level": "ERROR",
        "message": message,
        "event": "worker.bootstrap_error",
        "error_chain": format_error_chain(error),
        "error_source": format!("{error:?}"),
    });
    eprintln!("{line}");
}

fn format_error_chain(error: &anyhow::Error) -> String {
    error
        .chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ")
}
