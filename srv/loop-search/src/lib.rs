use anyhow::{Context, bail};
use chrono::{DateTime, Utc};
use loop_dto::{
    EventContentBlock, EventContentDoc, EventFeatureBlock, EventInlineNode, EventResp,
    EventSearchFacetResp, EventStatus, SearchEventsResp,
};
use loop_svc_model::event::Event;
use meilisearch_sdk::{
    client::Client,
    errors::{Error, ErrorCode},
    indexes::Index,
    search::Selectors,
    settings::{PaginationSetting, Settings},
    tasks::Task,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::Duration};

const DEFAULT_EVENT_INDEX_UID: &str = "events_v1";
const REINDEX_BATCH_SIZE: i64 = 500;
const TASK_POLL_INTERVAL: Duration = Duration::from_millis(100);
const TASK_TIMEOUT: Duration = Duration::from_secs(120);
const FACET_NAMES: [&str; 2] = ["tags", "location_name"];

#[derive(Debug)]
pub struct EventSearchParams {
    pub query: String,
    pub tags: Vec<String>,
    pub location: Option<String>,
    pub start_from: Option<DateTime<Utc>>,
    pub start_to: Option<DateTime<Utc>>,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Clone)]
pub struct EventSearchService {
    client: Client,
    index_uid: String,
}

/// 写入索引的文档刻意同时包含“返回字段”和“搜索辅助字段”。`content_text`
/// 保存从结构化富文本中抽取的完整可见文本，确保正文超过摘要长度后仍可命中。
#[derive(Debug, Serialize)]
struct EventSearchDocument {
    event_id: String,
    creator_id: String,
    title: String,
    content_version: i32,
    content: EventContentDoc,
    content_text: String,
    summary: String,
    cover_asset_id: Option<String>,
    start_at: Option<String>,
    end_at: Option<String>,
    start_at_timestamp: Option<i64>,
    end_at_timestamp: Option<i64>,
    location_name: Option<String>,
    location_address: Option<String>,
    capacity: Option<i32>,
    requires_approval: bool,
    tags: Vec<String>,
    created_at: String,
    updated_at: String,
    published_at_timestamp: Option<i64>,
}

/// 搜索响应只反序列化 displayed attributes。搜索辅助字段不会穿透到 HTTP API，
/// 避免把索引内部 schema 误当成客户端契约。
#[derive(Debug, Deserialize)]
struct EventSearchHit {
    event_id: String,
    creator_id: String,
    title: String,
    content_version: i32,
    content: EventContentDoc,
    summary: String,
    cover_asset_id: Option<String>,
    start_at: Option<String>,
    end_at: Option<String>,
    location_name: Option<String>,
    location_address: Option<String>,
    capacity: Option<i32>,
    requires_approval: bool,
    tags: Vec<String>,
    created_at: String,
    updated_at: String,
}

impl EventSearchService {
    /// API 与 worker 都从同一个环境变量读取版本化索引名，避免两个进程因
    /// TOML 配置不同而分别读写不同索引。
    pub fn from_env() -> anyhow::Result<Self> {
        let index_uid = std::env::var("LOOP_EVENT_SEARCH_INDEX_UID")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| DEFAULT_EVENT_INDEX_UID.to_string());
        validate_index_uid(&index_uid)?;
        Ok(Self {
            client: loop_infra::search::meilisearch_client()?.clone(),
            index_uid,
        })
    }

    #[tracing::instrument(
        name = "search.event.reader_initialize",
        skip_all,
        fields(search.index = %self.index_uid)
    )]
    pub async fn initialize_reader(&self) -> anyhow::Result<()> {
        let index = self.ensure_index().await?;
        self.configure_index(&index).await
    }

    #[tracing::instrument(
        name = "search.event.writer_initialize",
        skip_all,
        fields(search.index = %self.index_uid)
    )]
    pub async fn initialize_writer_and_sync(&self) -> anyhow::Result<()> {
        let index = self.ensure_index().await?;
        self.configure_index(&index).await?;
        self.sync_published_events(&index).await
    }

    async fn ensure_index(&self) -> anyhow::Result<Index> {
        match self.client.get_index(&self.index_uid).await {
            Ok(index) => {
                tracing::info!(
                    event = "search.event.index_found",
                    search_index = %self.index_uid,
                    "event search index already exists"
                );
                Ok(index)
            }
            Err(Error::Meilisearch(error)) if error.error_code == ErrorCode::IndexNotFound => {
                tracing::info!(
                    event = "search.event.index_create",
                    search_index = %self.index_uid,
                    "creating event search index"
                );
                let task = self
                    .client
                    .create_index(&self.index_uid, Some("event_id"))
                    .await
                    .context("failed to enqueue event index creation")?;
                wait_for_task(&self.client, task, "create event index").await?;
                self.client
                    .get_index(&self.index_uid)
                    .await
                    .context("failed to load newly created event index")
            }
            Err(error) => Err(error).context("failed to query event search index"),
        }
    }

    async fn configure_index(&self, index: &Index) -> anyhow::Result<()> {
        // 属性顺序也代表匹配优先级：标题优先于标签、地点和完整正文。
        let settings = Settings::new()
            .with_searchable_attributes([
                "title",
                "tags",
                "location_name",
                "location_address",
                "summary",
                "content_text",
            ])
            .with_filterable_attributes([
                "tags",
                "location_name",
                "start_at_timestamp",
                "end_at_timestamp",
            ])
            .with_sortable_attributes(["start_at_timestamp", "published_at_timestamp"])
            .with_pagination(PaginationSetting {
                max_total_hits: 10_000,
            })
            .with_displayed_attributes([
                "event_id",
                "creator_id",
                "title",
                "content_version",
                "content",
                "summary",
                "cover_asset_id",
                "start_at",
                "end_at",
                "location_name",
                "location_address",
                "capacity",
                "requires_approval",
                "tags",
                "created_at",
                "updated_at",
            ]);
        let task = index
            .set_settings(&settings)
            .await
            .context("failed to enqueue event index settings update")?;
        wait_for_task(&self.client, task, "configure event index").await?;
        tracing::info!(
            event = "search.event.index_configured",
            search_index = %self.index_uid,
            "event search index settings configured"
        );
        Ok(())
    }

    async fn sync_published_events(&self, index: &Index) -> anyhow::Result<()> {
        // 启动同步只做幂等 upsert，不清空共享索引。多个 worker 滚动启动时如果
        // 各自清空索引，会制造搜索空窗，并可能删掉其他 worker 刚写入的文档。
        let pool = loop_infra::db::pg_pool().context("failed to get PostgreSQL pool")?;
        let mut conn = pool
            .get()
            .await
            .context("failed to get PostgreSQL connection for event index sync")?;
        let mut after_id = 0_i64;
        let mut indexed_count = 0_usize;
        loop {
            let events = Event::select_published_after_id(after_id, REINDEX_BATCH_SIZE, &mut conn)
                .await
                .context("failed to load published events for index sync")?;
            if events.is_empty() {
                break;
            }
            after_id = events
                .last()
                .map(|event| event.event_id)
                .unwrap_or(after_id);
            let documents = events
                .iter()
                .map(EventSearchDocument::try_from)
                .collect::<anyhow::Result<Vec<_>>>()?;
            let task = index
                .add_documents(&documents, Some("event_id"))
                .await
                .context("failed to enqueue event index sync batch")?;
            wait_for_task(&self.client, task, "index event batch").await?;
            indexed_count += documents.len();
            tracing::info!(
                event = "search.event.sync_batch_completed",
                search_index = %self.index_uid,
                batch_count = documents.len(),
                indexed_count,
                after_event_id = after_id,
                "event search index sync batch completed"
            );
        }
        tracing::info!(
            event = "search.event.sync_completed",
            search_index = %self.index_uid,
            indexed_count,
            "published events synchronized to search index"
        );
        Ok(())
    }

    #[tracing::instrument(
        name = "search.event.apply",
        skip_all,
        fields(search.index = %self.index_uid, event.id = event.event_id, event.status = %event.status)
    )]
    pub async fn apply_event(&self, event: &Event) -> anyhow::Result<()> {
        let index = self.client.index(&self.index_uid);
        let task = if event.status == "published" {
            let document = EventSearchDocument::try_from(event)?;
            index
                .add_documents(&[document], Some("event_id"))
                .await
                .context("failed to enqueue published event indexing")?
        } else {
            // 消息只携带 event_id，consumer 总是读取数据库最新状态。因此即使
            // publish/cancel 消息重复或乱序，最终动作仍与 PostgreSQL 当前状态一致。
            index
                .delete_document(event.event_id.to_string())
                .await
                .context("failed to enqueue event removal from search index")?
        };
        wait_for_task(&self.client, task, "apply event search document").await?;
        tracing::info!(
            event = "search.event.applied",
            search_index = %self.index_uid,
            event_id = event.event_id,
            event_status = %event.status,
            "event search document applied"
        );
        Ok(())
    }

    #[tracing::instrument(
        name = "search.event.remove",
        skip_all,
        fields(search.index = %self.index_uid, event.id = event_id)
    )]
    pub async fn remove_event(&self, event_id: i64) -> anyhow::Result<()> {
        let task = self
            .client
            .index(&self.index_uid)
            .delete_document(event_id.to_string())
            .await
            .context("failed to enqueue missing event removal from search index")?;
        wait_for_task(&self.client, task, "remove missing event search document").await?;
        tracing::info!(
            event = "search.event.removed",
            search_index = %self.index_uid,
            event_id,
            "missing event removed from search index"
        );
        Ok(())
    }

    #[tracing::instrument(
        name = "search.event.query",
        skip_all,
        fields(
            search.index = %self.index_uid,
            search.query_length = params.query.chars().count(),
            search.tag_count = params.tags.len(),
            search.has_location = params.location.is_some(),
            search.has_start_from = params.start_from.is_some(),
            search.has_start_to = params.start_to.is_some(),
            search.limit = params.limit,
            search.offset = params.offset,
        )
    )]
    pub async fn search(&self, params: EventSearchParams) -> anyhow::Result<SearchEventsResp> {
        let filter = build_filter(&params)?;
        let index = self.client.index(&self.index_uid);
        let mut query = index.search();
        query
            .with_query(&params.query)
            .with_limit(params.limit)
            .with_offset(params.offset)
            .with_facets(Selectors::Some(&FACET_NAMES));
        if !filter.is_empty() {
            query.with_filter(&filter);
        }

        let result = query
            .execute::<EventSearchHit>()
            .await
            .context("failed to search events")?;
        let estimated_total = result.estimated_total_hits.unwrap_or(result.hits.len());
        let returned_count = result.hits.len();
        let next_offset = (params.offset + returned_count < estimated_total)
            .then_some((params.offset + returned_count) as i32);
        let mut facets = result.facet_distribution.unwrap_or_default();
        let tags = take_facets(&mut facets, "tags");
        let locations = take_facets(&mut facets, "location_name");
        let items = result
            .hits
            .into_iter()
            .map(|hit| hit.result.into_event_resp())
            .collect();
        tracing::info!(
            event = "search.event.query_completed",
            search_index = %self.index_uid,
            result_count = returned_count,
            estimated_total,
            processing_time_ms = result.processing_time_ms,
            "event search completed"
        );
        Ok(SearchEventsResp {
            items,
            next_offset,
            estimated_total: estimated_total.min(i32::MAX as usize) as i32,
            tags,
            locations,
        })
    }
}

impl TryFrom<&Event> for EventSearchDocument {
    type Error = anyhow::Error;

    fn try_from(event: &Event) -> Result<Self, Self::Error> {
        let content = serde_json::from_value::<EventContentDoc>(event.content_doc.clone())
            .with_context(|| format!("failed to parse event {} content", event.event_id))?;
        let content_text = extract_content_text(&content);
        Ok(Self {
            event_id: event.event_id.to_string(),
            creator_id: event.creator_id.to_string(),
            title: event.title.clone(),
            content_version: event.content_version,
            content,
            content_text,
            summary: event.summary.clone(),
            cover_asset_id: event.cover_asset_id.clone(),
            start_at: event.start_at.map(|value| value.to_rfc3339()),
            end_at: event.end_at.map(|value| value.to_rfc3339()),
            start_at_timestamp: event.start_at.map(|value| value.timestamp()),
            end_at_timestamp: event.end_at.map(|value| value.timestamp()),
            location_name: event.location_name.clone(),
            location_address: event.location_address.clone(),
            capacity: event.capacity,
            requires_approval: event.requires_approval,
            tags: event.tags.clone(),
            created_at: event.created_at.to_rfc3339(),
            updated_at: event.updated_at.to_rfc3339(),
            published_at_timestamp: event.published_at.map(|value| value.timestamp()),
        })
    }
}

impl EventSearchHit {
    fn into_event_resp(self) -> EventResp {
        EventResp {
            event_id: self.event_id,
            creator_id: self.creator_id,
            title: self.title,
            status: EventStatus::Published,
            content_version: self.content_version,
            content: self.content,
            summary: self.summary,
            cover_asset_id: self.cover_asset_id,
            start_at: self.start_at,
            end_at: self.end_at,
            location_name: self.location_name,
            location_address: self.location_address,
            capacity: self.capacity,
            requires_approval: self.requires_approval,
            tags: self.tags,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

fn validate_index_uid(value: &str) -> anyhow::Result<()> {
    if value.len() > 400
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        bail!("LOOP_EVENT_SEARCH_INDEX_UID must contain only ASCII letters, numbers, '-' or '_'");
    }
    Ok(())
}

fn build_filter(params: &EventSearchParams) -> anyhow::Result<String> {
    let mut filters = Vec::new();
    for tag in &params.tags {
        filters.push(format!("tags = {}", quote_filter_value(tag)?));
    }
    if let Some(location) = &params.location {
        filters.push(format!("location_name = {}", quote_filter_value(location)?));
    }
    if let Some(start_from) = params.start_from {
        filters.push(format!("start_at_timestamp >= {}", start_from.timestamp()));
    }
    if let Some(start_to) = params.start_to {
        filters.push(format!("start_at_timestamp <= {}", start_to.timestamp()));
    }
    Ok(filters.join(" AND "))
}

fn quote_filter_value(value: &str) -> anyhow::Result<String> {
    serde_json::to_string(value).context("failed to quote Meilisearch filter value")
}

fn take_facets(
    facets: &mut HashMap<String, HashMap<String, usize>>,
    name: &str,
) -> Vec<EventSearchFacetResp> {
    let mut values = facets
        .remove(name)
        .unwrap_or_default()
        .into_iter()
        .map(|(value, count)| EventSearchFacetResp {
            value,
            count: count.min(i32::MAX as usize) as i32,
        })
        .collect::<Vec<_>>();
    values.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.value.cmp(&right.value))
    });
    values
}

fn extract_content_text(content: &EventContentDoc) -> String {
    let mut parts = Vec::new();
    for block in &content.blocks {
        match block {
            EventContentBlock::Heading { children, .. }
            | EventContentBlock::Paragraph { children, .. }
            | EventContentBlock::Quote { children, .. } => {
                parts.extend(children.iter().map(inline_text));
            }
            EventContentBlock::Image { item, caption, .. } => {
                if let Some(alt) = &item.alt {
                    parts.push(alt.as_str());
                }
                if let Some(caption) = caption {
                    parts.push(caption.as_str());
                }
            }
            EventContentBlock::ImageGrid { items, .. } => {
                parts.extend(items.iter().filter_map(|item| item.alt.as_deref()));
            }
            EventContentBlock::Divider { .. } => {}
            EventContentBlock::Feature { feature, .. } => match feature {
                EventFeatureBlock::Schedule { title, items }
                | EventFeatureBlock::Notice { title, items } => {
                    parts.push(title.as_str());
                    parts.extend(items.iter().map(String::as_str));
                }
                EventFeatureBlock::Location { title, address } => {
                    parts.push(title.as_str());
                    if let Some(address) = address {
                        parts.push(address.as_str());
                    }
                }
                EventFeatureBlock::Ticket { title, description } => {
                    parts.push(title.as_str());
                    parts.push(description.as_str());
                }
            },
        }
    }
    parts.join(" ")
}

fn inline_text(node: &EventInlineNode) -> &str {
    match node {
        EventInlineNode::Text { text, .. }
        | EventInlineNode::Hashtag { text, .. }
        | EventInlineNode::Link { text, .. } => text,
        EventInlineNode::Mention { label, .. } => label,
    }
}

async fn wait_for_task(
    client: &Client,
    task: meilisearch_sdk::task_info::TaskInfo,
    operation: &str,
) -> anyhow::Result<()> {
    let task = task
        .wait_for_completion(client, Some(TASK_POLL_INTERVAL), Some(TASK_TIMEOUT))
        .await
        .with_context(|| format!("timed out waiting to {operation}"))?;
    if let Task::Failed { .. } = &task {
        let error = task.unwrap_failure();
        bail!("failed to {operation}: {error}");
    }
    Ok(())
}
