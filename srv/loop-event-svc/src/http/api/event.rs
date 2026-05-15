use crate::{
    db_conn,
    http::{AppRoutes, AuthInfo, HttpErr, ResultExt, err_key::*},
};
use axum::{
    Json,
    extract::{Query, State},
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use http::{Method, StatusCode};
use loop_dto::{
    CreateEventRequest, EventContentBlock, EventContentDoc, EventContentImage, EventFeatureBlock,
    EventInlineNode, EventResp, EventStatus, EventTextMark, ListEventsResp,
};
use loop_svc_model::event::{Event, NewEvent};
use serde::Deserialize;
use std::collections::HashSet;

const EVENT_CREATE_PERMISSION: &str = "event.create";
const DEFAULT_LIST_LIMIT: i32 = 20;
const MAX_LIST_LIMIT: i32 = 50;
const MAX_TITLE_CHARS: usize = 80;
const MAX_BLOCKS: usize = 100;
const MAX_INLINE_NODES_PER_PARAGRAPH: usize = 100;
const MAX_TEXT_CHARS: usize = 10_000;
const MAX_TOTAL_IMAGES: usize = 18;
const MAX_IMAGES_PER_GRID: usize = 9;
const MAX_CAPTION_CHARS: usize = 120;
const MAX_FEATURE_ITEMS: usize = 20;
const MAX_FEATURE_TEXT_CHARS: usize = 200;
const MAX_TAGS: usize = 10;
const MAX_TAG_CHARS: usize = 20;
const MAX_SUMMARY_CHARS: usize = 120;

#[derive(Debug, Deserialize)]
pub struct ListEventsQuery {
    limit: Option<i32>,
    offset: Option<i32>,
}

#[derive(Debug, Default)]
struct ContentStats {
    text_chars: usize,
    image_count: usize,
    plain_text: String,
}

#[tracing::instrument(
    name = "event.create",
    skip_all,
    fields(user.id = auth.user_id, event.id = tracing::field::Empty)
)]
pub async fn create_event(
    State(_state): State<crate::http::AppState>,
    auth: AuthInfo,
    Json(req): Json<CreateEventRequest>,
) -> Result<(StatusCode, Json<EventResp>), HttpErr> {
    let title = normalize_required_text(&req.title, MAX_TITLE_CHARS)?;
    let content_stats = validate_content(&req.content)?;
    let content_doc = serde_json::to_value(&req.content).internal(DB_ERROR)?;
    let tags = normalize_tags(req.tags)?;
    let start_at = parse_optional_time(req.start_at.as_deref())?;
    let end_at = parse_optional_time(req.end_at.as_deref())?;
    if start_at
        .zip(end_at)
        .is_some_and(|(start, end)| end <= start)
    {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }

    let event = NewEvent {
        creator_id: auth.user_id,
        title,
        status: "published".to_string(),
        content_doc,
        summary: build_summary(&content_stats.plain_text),
        cover_asset_id: cover_asset_id(&req.content),
        start_at,
        end_at,
        location_name: normalize_optional_text(req.location_name, 80)?,
        location_address: normalize_optional_text(req.location_address, 200)?,
        capacity: validate_capacity(req.capacity)?,
        tags,
        published_at: Some(Utc::now()),
    };

    let event = Event::insert(event, &mut db_conn!())
        .await
        .internal(DB_ERROR)?;
    tracing::Span::current().record("event.id", event.event_id);
    tracing::info!(
        event = "event.created",
        event_id = event.event_id,
        user_id = auth.user_id,
        "event created"
    );

    Ok((StatusCode::CREATED, Json(to_event_resp(event)?)))
}

#[tracing::instrument(name = "event.list", skip_all)]
pub async fn list_events(
    State(_state): State<crate::http::AppState>,
    Query(query): Query<ListEventsQuery>,
) -> Result<Json<ListEventsResp>, HttpErr> {
    let limit = query
        .limit
        .unwrap_or(DEFAULT_LIST_LIMIT)
        .clamp(1, MAX_LIST_LIMIT);
    let offset = query.offset.unwrap_or(0).max(0);
    let rows = Event::select_published(i64::from(limit), i64::from(offset), &mut db_conn!())
        .await
        .internal(DB_ERROR)?;
    let has_next = rows.len() == limit as usize;
    let items = rows
        .into_iter()
        .map(to_event_resp)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(ListEventsResp {
        items,
        next_offset: has_next.then_some(offset + limit),
    }))
}

fn to_event_resp(event: Event) -> Result<EventResp, HttpErr> {
    let content =
        serde_json::from_value::<EventContentDoc>(event.content_doc).internal(DB_ERROR)?;
    Ok(EventResp {
        event_id: event.event_id.to_string(),
        creator_id: event.creator_id.to_string(),
        title: event.title,
        status: match event.status.as_str() {
            "draft" => EventStatus::Draft,
            "published" => EventStatus::Published,
            "cancelled" => EventStatus::Cancelled,
            _ => {
                return Err(HttpErr::internal(
                    DB_ERROR,
                    anyhow::anyhow!("unknown event status"),
                ));
            }
        },
        content,
        summary: event.summary,
        cover_asset_id: event.cover_asset_id,
        start_at: event.start_at.map(|value| value.to_rfc3339()),
        end_at: event.end_at.map(|value| value.to_rfc3339()),
        location_name: event.location_name,
        location_address: event.location_address,
        capacity: event.capacity,
        tags: event.tags,
        created_at: event.created_at.to_rfc3339(),
        updated_at: event.updated_at.to_rfc3339(),
    })
}

fn normalize_required_text(value: &str, max_chars: usize) -> Result<String, HttpErr> {
    let normalized = value.trim().to_string();
    if normalized.is_empty() || normalized.chars().count() > max_chars {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    Ok(normalized)
}

fn normalize_optional_text(
    value: Option<String>,
    max_chars: usize,
) -> Result<Option<String>, HttpErr> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| normalize_required_text(&value, max_chars))
        .transpose()
}

fn normalize_tags(tags: Vec<String>) -> Result<Vec<String>, HttpErr> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for tag in tags {
        let tag = tag.trim().trim_start_matches('#').to_string();
        if tag.is_empty() {
            continue;
        }
        if tag.chars().count() > MAX_TAG_CHARS || !seen.insert(tag.clone()) {
            continue;
        }
        normalized.push(tag);
        if normalized.len() > MAX_TAGS {
            return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
        }
    }
    Ok(normalized)
}

fn validate_capacity(capacity: Option<i32>) -> Result<Option<i32>, HttpErr> {
    if capacity.is_some_and(|capacity| capacity <= 0) {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    Ok(capacity)
}

fn parse_optional_time(value: Option<&str>) -> Result<Option<DateTime<Utc>>, HttpErr> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    DateTime::parse_from_rfc3339(value)
        .map(|value| Some(value.with_timezone(&Utc)))
        .map_err(|_| HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT))
}

fn validate_content(content: &EventContentDoc) -> Result<ContentStats, HttpErr> {
    if !matches!(content.schema_version, 1 | 2)
        || content.blocks.is_empty()
        || content.blocks.len() > MAX_BLOCKS
    {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }

    let mut stats = ContentStats::default();
    let mut block_ids = HashSet::new();
    for block in &content.blocks {
        match block {
            EventContentBlock::Heading {
                id,
                level,
                children,
            } => {
                validate_block_id(id, &mut block_ids)?;
                if !(1..=3).contains(level) {
                    return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
                }
                validate_text_children(children, &mut stats)?;
            }
            EventContentBlock::Paragraph { id, children } => {
                validate_block_id(id, &mut block_ids)?;
                validate_text_children(children, &mut stats)?;
            }
            EventContentBlock::Quote { id, children } => {
                validate_block_id(id, &mut block_ids)?;
                validate_text_children(children, &mut stats)?;
            }
            EventContentBlock::Image { id, item, caption } => {
                validate_block_id(id, &mut block_ids)?;
                validate_image_item(item, &mut stats)?;
                validate_optional_block_text(caption.as_deref(), MAX_CAPTION_CHARS, &mut stats)?;
            }
            EventContentBlock::ImageGrid { id, items } => {
                validate_block_id(id, &mut block_ids)?;
                validate_image_grid(items, &mut stats)?;
            }
            EventContentBlock::Divider { id } => {
                validate_block_id(id, &mut block_ids)?;
            }
            EventContentBlock::Feature { id, feature } => {
                validate_block_id(id, &mut block_ids)?;
                validate_feature(feature, &mut stats)?;
            }
        }
    }

    if stats.text_chars == 0 && stats.image_count == 0 {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    Ok(stats)
}

fn validate_block_id(id: &str, seen: &mut HashSet<String>) -> Result<(), HttpErr> {
    let id = id.trim();
    if id.is_empty() || id.chars().count() > 64 || !seen.insert(id.to_string()) {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    Ok(())
}

fn validate_text_children(
    children: &[EventInlineNode],
    stats: &mut ContentStats,
) -> Result<(), HttpErr> {
    if children.is_empty() || children.len() > MAX_INLINE_NODES_PER_PARAGRAPH {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }

    for child in children {
        let visible_text = match child {
            EventInlineNode::Text { text, marks } => {
                validate_text_marks(marks)?;
                text.as_str()
            }
            EventInlineNode::Hashtag { text, .. } => {
                if text.chars().count() > MAX_TAG_CHARS + 1 {
                    return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
                }
                text.as_str()
            }
            EventInlineNode::Mention { user_id, label } => {
                if user_id.trim().is_empty() {
                    return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
                }
                label.as_str()
            }
            EventInlineNode::Link { text, url } => {
                if text.trim().is_empty()
                    || url.chars().count() > 500
                    || (!url.starts_with("http://") && !url.starts_with("https://"))
                {
                    return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
                }
                text.as_str()
            }
        };
        add_plain_text(stats, visible_text, MAX_TEXT_CHARS)?;
    }
    Ok(())
}

fn validate_text_marks(marks: &[EventTextMark]) -> Result<(), HttpErr> {
    let mut bold = false;
    let mut italic = false;
    let mut underline = false;
    let mut color = false;
    for mark in marks {
        let duplicated = match mark {
            EventTextMark::Bold => std::mem::replace(&mut bold, true),
            EventTextMark::Italic => std::mem::replace(&mut italic, true),
            EventTextMark::Underline => std::mem::replace(&mut underline, true),
            EventTextMark::Color { .. } => std::mem::replace(&mut color, true),
        };
        if duplicated {
            return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
        }
    }
    Ok(())
}

fn validate_image_item(item: &EventContentImage, stats: &mut ContentStats) -> Result<(), HttpErr> {
    stats.image_count += 1;
    if stats.image_count > MAX_TOTAL_IMAGES
        || item.asset_id.trim().is_empty()
        || item.asset_id.chars().count() > 128
        || item.width <= 0
        || item.height <= 0
    {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    Ok(())
}

fn validate_image_grid(
    items: &[EventContentImage],
    stats: &mut ContentStats,
) -> Result<(), HttpErr> {
    if items.is_empty() || items.len() > MAX_IMAGES_PER_GRID {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    for item in items {
        validate_image_item(item, stats)?;
    }
    Ok(())
}

fn validate_feature(feature: &EventFeatureBlock, stats: &mut ContentStats) -> Result<(), HttpErr> {
    match feature {
        EventFeatureBlock::Schedule { title, items }
        | EventFeatureBlock::Notice { title, items } => {
            validate_required_block_text(title, MAX_FEATURE_TEXT_CHARS, stats)?;
            validate_feature_items(items, stats)?;
        }
        EventFeatureBlock::Location { title, address } => {
            validate_required_block_text(title, MAX_FEATURE_TEXT_CHARS, stats)?;
            validate_optional_block_text(address.as_deref(), MAX_FEATURE_TEXT_CHARS, stats)?;
        }
        EventFeatureBlock::Ticket { title, description } => {
            validate_required_block_text(title, MAX_FEATURE_TEXT_CHARS, stats)?;
            validate_required_block_text(description, MAX_FEATURE_TEXT_CHARS, stats)?;
        }
    }
    Ok(())
}

fn validate_feature_items(items: &[String], stats: &mut ContentStats) -> Result<(), HttpErr> {
    if items.is_empty() || items.len() > MAX_FEATURE_ITEMS {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    for item in items {
        validate_required_block_text(item, MAX_FEATURE_TEXT_CHARS, stats)?;
    }
    Ok(())
}

fn validate_required_block_text(
    value: &str,
    max_chars: usize,
    stats: &mut ContentStats,
) -> Result<(), HttpErr> {
    let normalized = value.trim();
    if normalized.is_empty() || normalized.chars().count() > max_chars {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    add_plain_text(stats, normalized, MAX_TEXT_CHARS)
}

fn validate_optional_block_text(
    value: Option<&str>,
    max_chars: usize,
    stats: &mut ContentStats,
) -> Result<(), HttpErr> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    if value.chars().count() > max_chars {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    add_plain_text(stats, value, MAX_TEXT_CHARS)
}

fn add_plain_text(
    stats: &mut ContentStats,
    value: &str,
    max_total_chars: usize,
) -> Result<(), HttpErr> {
    let value = value.trim();
    let text_chars = value.chars().count();
    stats.text_chars += text_chars;
    if stats.text_chars > max_total_chars {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    if !value.is_empty() {
        if !stats.plain_text.is_empty() {
            stats.plain_text.push(' ');
        }
        stats.plain_text.push_str(value);
    }
    Ok(())
}

fn build_summary(plain_text: &str) -> String {
    plain_text.chars().take(MAX_SUMMARY_CHARS).collect()
}

fn cover_asset_id(content: &EventContentDoc) -> Option<String> {
    content.blocks.iter().find_map(|block| match block {
        EventContentBlock::Image { item, .. } => Some(item.asset_id.trim().to_string()),
        EventContentBlock::ImageGrid { items, .. } => {
            items.first().map(|item| item.asset_id.trim().to_string())
        }
        EventContentBlock::Divider { .. }
        | EventContentBlock::Feature { .. }
        | EventContentBlock::Heading { .. }
        | EventContentBlock::Paragraph { .. }
        | EventContentBlock::Quote { .. } => None,
    })
}

pub fn route(state: crate::http::AppState) -> AppRoutes {
    AppRoutes::<crate::http::AppState>::new()
        .public(Method::GET, "/event", get(list_events))
        .protected(
            Method::POST,
            "/event",
            EVENT_CREATE_PERMISSION,
            post(create_event),
        )
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_doc(text: &str) -> EventContentDoc {
        EventContentDoc {
            schema_version: 1,
            blocks: vec![EventContentBlock::Paragraph {
                id: "p1".to_string(),
                children: vec![EventInlineNode::Text {
                    text: text.to_string(),
                    marks: Vec::new(),
                }],
            }],
        }
    }

    #[test]
    fn validate_content_accepts_text_doc() {
        let stats = validate_content(&text_doc("活动介绍")).expect("content should be valid");
        assert_eq!(stats.text_chars, 4);
        assert_eq!(stats.image_count, 0);
    }

    #[test]
    fn validate_content_rejects_empty_text() {
        validate_content(&text_doc("   ")).expect_err("empty content should be rejected");
    }
}
