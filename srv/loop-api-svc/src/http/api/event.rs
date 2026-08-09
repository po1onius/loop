use crate::{
    db_conn,
    http::{AppRoutes, AuthInfo, HttpErr, ResultExt, err_key::*},
};
use axum::{
    Json,
    extract::{Path, Query, State},
    routing::{delete, get, patch, post},
};
use chrono::{DateTime, Utc};
use diesel_async::AsyncConnection;
use http::{Method, StatusCode};
use loop_dto::{
    CreateEventDraftRequest, CreateEventRequest, EVENT_CONTENT_VERSION_V1, EventContentBlock,
    EventContentDoc, EventContentImage, EventFeatureBlock, EventInlineNode, EventJoinRequestResp,
    EventJoinReviewDecision, EventParticipationResp, EventParticipationStateResp,
    EventParticipationStatus, EventResp, EventStatus, EventTextMark, ListEventJoinRequestsResp,
    ListEventsResp, ReviewEventJoinRequest, UpdateEventDraftRequest,
};
use loop_svc_model::{
    DieselConn,
    event::{
        Event, EventDraftChanges, EventParticipation, MediaAsset, NewEvent, NewEventParticipation,
        PublishEventDraftChanges,
    },
};
use serde::Deserialize;
use std::collections::HashSet;

const EVENT_CREATE_PERMISSION: &str = "event.create";
const EVENT_JOIN_PERMISSION: &str = "event.join";
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

#[derive(Debug, Deserialize)]
pub struct ListOwnedEventsQuery {
    status: Option<String>,
    limit: Option<i32>,
    offset: Option<i32>,
}

#[derive(Debug, Default)]
struct ContentStats {
    text_chars: usize,
    image_count: usize,
    plain_text: String,
}

struct NormalizedEventDraft {
    title: String,
    content: EventContentDoc,
    content_stats: ContentStats,
    asset_ids: HashSet<String>,
    tags: Vec<String>,
    start_at: Option<DateTime<Utc>>,
    end_at: Option<DateTime<Utc>>,
    location_name: Option<String>,
    location_address: Option<String>,
    capacity: Option<i32>,
    requires_approval: bool,
}

// 发布、首次保存和覆盖保存共享同一套规范化逻辑。用具名载荷承载原始字段，
// 避免多个位置依赖易错的长参数顺序，也便于以后为草稿字段增加统一校验。
struct RawEventPayload {
    title: String,
    content: EventContentDoc,
    start_at: Option<String>,
    end_at: Option<String>,
    location_name: Option<String>,
    location_address: Option<String>,
    capacity: Option<i32>,
    requires_approval: bool,
    tags: Vec<String>,
    mode: ContentValidationMode,
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
    let draft = normalize_publish_payload(req)?;
    validate_referenced_media_assets(auth.user_id, &draft.asset_ids).await?;
    let event = insert_event(auth.user_id, "published", draft, Some(Utc::now())).await?;
    tracing::Span::current().record("event.id", event.event_id);
    tracing::info!(
        event = "event.created",
        event_id = event.event_id,
        user_id = auth.user_id,
        "event created and published"
    );

    Ok((StatusCode::CREATED, Json(to_event_resp(event)?)))
}

#[tracing::instrument(
    name = "event.draft.create",
    skip_all,
    fields(user.id = auth.user_id, event.id = tracing::field::Empty)
)]
pub async fn create_event_draft(
    State(_state): State<crate::http::AppState>,
    auth: AuthInfo,
    Json(req): Json<CreateEventDraftRequest>,
) -> Result<(StatusCode, Json<EventResp>), HttpErr> {
    let draft = normalize_draft_create_payload(req)?;
    validate_referenced_media_assets(auth.user_id, &draft.asset_ids).await?;
    let event = insert_event(auth.user_id, "draft", draft, None).await?;
    tracing::Span::current().record("event.id", event.event_id);
    tracing::info!(
        event = "event.draft_created",
        event_id = event.event_id,
        user_id = auth.user_id,
        "event draft created"
    );

    Ok((StatusCode::CREATED, Json(to_event_resp(event)?)))
}

#[tracing::instrument(
    name = "event.draft.update",
    skip_all,
    fields(user.id = auth.user_id, event.id = %event_id)
)]
pub async fn update_event_draft(
    State(_state): State<crate::http::AppState>,
    auth: AuthInfo,
    Path(event_id): Path<String>,
    Json(req): Json<UpdateEventDraftRequest>,
) -> Result<Json<EventResp>, HttpErr> {
    let event_id = parse_event_id(&event_id)?;
    let draft = normalize_draft_update_payload(req)?;
    validate_referenced_media_assets(auth.user_id, &draft.asset_ids).await?;
    let changes = to_draft_changes(draft)?;
    // 草稿不使用版本号或编辑租约；数据库最后提交的保存请求覆盖此前内容。
    let event = Event::update_draft(event_id, auth.user_id, changes, &mut db_conn!())
        .await
        .map_err(map_event_mutation_error)?;
    tracing::info!(
        event = "event.draft_updated",
        event_id = event.event_id,
        user_id = auth.user_id,
        "event draft snapshot updated with last-write-wins semantics"
    );

    Ok(Json(to_event_resp(event)?))
}

#[tracing::instrument(
    name = "event.draft.publish",
    skip_all,
    fields(user.id = auth.user_id, event.id = %event_id)
)]
pub async fn publish_event_draft(
    State(_state): State<crate::http::AppState>,
    auth: AuthInfo,
    Path(event_id): Path<String>,
) -> Result<Json<EventResp>, HttpErr> {
    let event_id = parse_event_id(&event_id)?;
    let mut conn = db_conn!();
    let event = conn
        .transaction::<Event, HttpErr, _>(async |conn| {
            let event = Event::select_owned_draft_for_update(event_id, auth.user_id, conn)
                .await
                .internal(DB_ERROR)?
                .ok_or_else(|| HttpErr::client(StatusCode::NOT_FOUND, EVENT_NOT_FOUND))?;
            publish_locked_event_draft(auth.user_id, event, conn).await
        })
        .await?;
    tracing::info!(
        event = "event.draft_published",
        event_id = event.event_id,
        user_id = auth.user_id,
        "event draft published"
    );

    Ok(Json(to_event_resp(event)?))
}

#[tracing::instrument(
    name = "event.draft.delete",
    skip_all,
    fields(user.id = auth.user_id, event.id = %event_id)
)]
pub async fn delete_event_draft(
    State(_state): State<crate::http::AppState>,
    auth: AuthInfo,
    Path(event_id): Path<String>,
) -> Result<StatusCode, HttpErr> {
    let event_id = parse_event_id(&event_id)?;
    let affected = Event::delete_draft(event_id, auth.user_id, &mut db_conn!())
        .await
        .internal(DB_ERROR)?;
    if affected == 0 {
        return Err(HttpErr::client(StatusCode::NOT_FOUND, EVENT_NOT_FOUND));
    }
    tracing::info!(
        event = "event.draft_deleted",
        event_id = event_id,
        user_id = auth.user_id,
        "event draft deleted"
    );
    Ok(StatusCode::NO_CONTENT)
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

#[tracing::instrument(name = "event.owned.list", skip_all, fields(user.id = auth.user_id))]
pub async fn list_owned_events(
    State(_state): State<crate::http::AppState>,
    auth: AuthInfo,
    Query(query): Query<ListOwnedEventsQuery>,
) -> Result<Json<ListEventsResp>, HttpErr> {
    let limit = query
        .limit
        .unwrap_or(DEFAULT_LIST_LIMIT)
        .clamp(1, MAX_LIST_LIMIT);
    let offset = query.offset.unwrap_or(0).max(0);
    let status = parse_optional_status(query.status.as_deref())?;
    let rows = Event::select_owned(
        auth.user_id,
        status.as_deref(),
        i64::from(limit),
        i64::from(offset),
        &mut db_conn!(),
    )
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

#[tracing::instrument(
    name = "event.joined.list",
    skip_all,
    fields(user.id = auth.user_id)
)]
pub async fn list_joined_events(
    State(_state): State<crate::http::AppState>,
    auth: AuthInfo,
    Query(query): Query<ListEventsQuery>,
) -> Result<Json<ListEventsResp>, HttpErr> {
    let limit = query
        .limit
        .unwrap_or(DEFAULT_LIST_LIMIT)
        .clamp(1, MAX_LIST_LIMIT);
    let offset = query.offset.unwrap_or(0).max(0);
    let rows = Event::select_joined_by_user(
        auth.user_id,
        i64::from(limit),
        i64::from(offset),
        &mut db_conn!(),
    )
    .await
    .internal(DB_ERROR)?;
    let has_next = rows.len() == limit as usize;
    let items = rows
        .into_iter()
        .map(to_event_resp)
        .collect::<Result<Vec<_>, _>>()?;

    tracing::info!(
        event = "event.joined_list_loaded",
        user_id = auth.user_id,
        item_count = items.len(),
        limit,
        offset,
        "joined event list loaded"
    );
    Ok(Json(ListEventsResp {
        items,
        next_offset: has_next.then_some(offset + limit),
    }))
}

#[tracing::instrument(
    name = "event.get",
    skip_all,
    fields(event.id = %event_id)
)]
pub async fn get_event(
    State(_state): State<crate::http::AppState>,
    Path(event_id): Path<String>,
) -> Result<Json<EventResp>, HttpErr> {
    let event_id = parse_event_id(&event_id)?;

    // 公开详情接口只允许读取已发布活动，避免草稿或已取消活动通过 ID 被公开访问。
    let event = Event::select_published_by_id(event_id, &mut db_conn!())
        .await
        .internal(DB_ERROR)?
        .ok_or_else(|| HttpErr::client(StatusCode::NOT_FOUND, EVENT_NOT_FOUND))?;
    tracing::info!(
        event = "event.detail_loaded",
        event_id = event.event_id,
        creator_id = event.creator_id,
        "event detail loaded"
    );

    Ok(Json(to_event_resp(event)?))
}

#[tracing::instrument(
    name = "event.participation.get",
    skip_all,
    fields(user.id = auth.user_id, event.id = %event_id)
)]
pub async fn get_event_participation(
    State(_state): State<crate::http::AppState>,
    auth: AuthInfo,
    Path(event_id): Path<String>,
) -> Result<Json<EventParticipationStateResp>, HttpErr> {
    let event_id = parse_event_id(&event_id)?;
    let mut conn = db_conn!();
    let event = Event::select_published_by_id(event_id, &mut conn)
        .await
        .internal(DB_ERROR)?
        .ok_or_else(|| HttpErr::client(StatusCode::NOT_FOUND, EVENT_NOT_FOUND))?;
    let participation = EventParticipation::select(event_id, auth.user_id, &mut conn)
        .await
        .internal(DB_ERROR)?
        .map(to_event_participation_resp)
        .transpose()?;
    let is_creator = event.creator_id == auth.user_id;

    tracing::info!(
        event = "event.participation_state_loaded",
        event_id,
        user_id = auth.user_id,
        is_creator,
        participation_status = participation
            .as_ref()
            .map(|item| participation_status_name(&item.status))
            .unwrap_or("none"),
        "event participation state loaded"
    );
    Ok(Json(EventParticipationStateResp {
        is_creator,
        participation,
    }))
}

#[tracing::instrument(
    name = "event.join",
    skip_all,
    fields(user.id = auth.user_id, event.id = %event_id, participation.status = tracing::field::Empty)
)]
pub async fn join_event(
    State(_state): State<crate::http::AppState>,
    auth: AuthInfo,
    Path(event_id): Path<String>,
) -> Result<Json<EventParticipationResp>, HttpErr> {
    let event_id = parse_event_id(&event_id)?;
    let mut conn = db_conn!();
    let participation = conn
        .transaction::<EventParticipation, HttpErr, _>(async |conn| {
            let event = Event::select_published_by_id_for_update(event_id, conn)
                .await
                .internal(DB_ERROR)?
                .ok_or_else(|| HttpErr::client(StatusCode::NOT_FOUND, EVENT_NOT_FOUND))?;
            if event.creator_id == auth.user_id {
                tracing::warn!(
                    event = "event.join_creator_rejected",
                    event_id,
                    user_id = auth.user_id,
                    "event creator cannot join their own event"
                );
                return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
            }

            if let Some(existing) = EventParticipation::select(event_id, auth.user_id, conn)
                .await
                .internal(DB_ERROR)?
            {
                // pending/joined 的重复请求直接返回现有状态；被拒绝后允许用户重新申请。
                if existing.status != "rejected" {
                    return Ok(existing);
                }
                let now = Utc::now();
                let next_status = if event.requires_approval {
                    "pending"
                } else {
                    ensure_event_capacity_available(&event, conn).await?;
                    "joined"
                };
                let joined_at = (next_status == "joined").then_some(now);
                return EventParticipation::reapply(
                    event_id,
                    auth.user_id,
                    next_status,
                    now,
                    joined_at,
                    conn,
                )
                .await
                .internal(DB_ERROR);
            }

            let now = Utc::now();
            let status = if event.requires_approval {
                "pending"
            } else {
                ensure_event_capacity_available(&event, conn).await?;
                "joined"
            };
            EventParticipation::insert(
                NewEventParticipation {
                    event_id,
                    user_id: auth.user_id,
                    status: status.to_string(),
                    requested_at: now,
                    reviewed_at: None,
                    reviewed_by: None,
                    joined_at: (status == "joined").then_some(now),
                },
                conn,
            )
            .await
            .internal(DB_ERROR)
        })
        .await?;

    tracing::Span::current().record("participation.status", &participation.status);
    tracing::info!(
        event = "event.join_completed",
        event_id,
        user_id = auth.user_id,
        participation_status = %participation.status,
        "event join request completed"
    );
    Ok(Json(to_event_participation_resp(participation)?))
}

#[tracing::instrument(
    name = "event.join_requests.list",
    skip_all,
    fields(user.id = auth.user_id, event.id = %event_id)
)]
pub async fn list_event_join_requests(
    State(_state): State<crate::http::AppState>,
    auth: AuthInfo,
    Path(event_id): Path<String>,
) -> Result<Json<ListEventJoinRequestsResp>, HttpErr> {
    let event_id = parse_event_id(&event_id)?;
    let mut conn = db_conn!();
    let event = Event::select_owned_by_id(event_id, auth.user_id, &mut conn)
        .await
        .internal(DB_ERROR)?
        .filter(|event| event.status == "published")
        .ok_or_else(|| HttpErr::client(StatusCode::NOT_FOUND, EVENT_NOT_FOUND))?;
    let rows = EventParticipation::list_pending_applicants(event.event_id, &mut conn)
        .await
        .internal(DB_ERROR)?;
    let items = rows
        .into_iter()
        .map(|row| {
            Ok(EventJoinRequestResp {
                user_id: row.participation.user_id.to_string(),
                username: row.username,
                status: to_event_participation_status(&row.participation.status)?,
                requested_at: row.participation.requested_at.to_rfc3339(),
                reviewed_at: row
                    .participation
                    .reviewed_at
                    .map(|value| value.to_rfc3339()),
            })
        })
        .collect::<Result<Vec<_>, HttpErr>>()?;

    tracing::info!(
        event = "event.join_requests_loaded",
        event_id,
        user_id = auth.user_id,
        pending_count = items.len(),
        "pending event join requests loaded"
    );
    Ok(Json(ListEventJoinRequestsResp { items }))
}

#[tracing::instrument(
    name = "event.join_request.review",
    skip_all,
    fields(
        user.id = auth.user_id,
        event.id = %event_id,
        applicant.id = %applicant_id,
        participation.status = tracing::field::Empty,
    )
)]
pub async fn review_event_join_request(
    State(_state): State<crate::http::AppState>,
    auth: AuthInfo,
    Path((event_id, applicant_id)): Path<(String, String)>,
    Json(req): Json<ReviewEventJoinRequest>,
) -> Result<Json<EventParticipationResp>, HttpErr> {
    let event_id = parse_event_id(&event_id)?;
    let applicant_id = parse_user_id(&applicant_id)?;
    let mut conn = db_conn!();
    let participation = conn
        .transaction::<EventParticipation, HttpErr, _>(async |conn| {
            let event = Event::select_published_by_id_for_update(event_id, conn)
                .await
                .internal(DB_ERROR)?
                .filter(|event| event.creator_id == auth.user_id)
                .ok_or_else(|| HttpErr::client(StatusCode::NOT_FOUND, EVENT_NOT_FOUND))?;
            let existing = EventParticipation::select(event_id, applicant_id, conn)
                .await
                .internal(DB_ERROR)?
                .ok_or_else(|| {
                    HttpErr::client(StatusCode::NOT_FOUND, EVENT_JOIN_REQUEST_NOT_FOUND)
                })?;

            // 对相同最终状态的审核请求保持幂等；不允许用审核接口移除已加入成员，
            // 也不允许直接翻转一条已拒绝记录，用户需要重新提交申请。
            match (&req.decision, existing.status.as_str()) {
                (EventJoinReviewDecision::Approve, "joined")
                | (EventJoinReviewDecision::Reject, "rejected") => return Ok(existing),
                (_, "pending") => {}
                _ => return Err(HttpErr::client(StatusCode::CONFLICT, INVALID_INPUT)),
            }

            let (status, joined_at) = match req.decision {
                EventJoinReviewDecision::Approve => {
                    ensure_event_capacity_available(&event, conn).await?;
                    let now = Utc::now();
                    ("joined", Some(now))
                }
                EventJoinReviewDecision::Reject => ("rejected", None),
            };
            EventParticipation::review_pending(
                event_id,
                applicant_id,
                auth.user_id,
                status,
                Utc::now(),
                joined_at,
                conn,
            )
            .await
            .map_err(|err| match err {
                diesel::result::Error::NotFound => {
                    HttpErr::client(StatusCode::NOT_FOUND, EVENT_JOIN_REQUEST_NOT_FOUND)
                }
                err => HttpErr::internal(DB_ERROR, err),
            })
        })
        .await?;

    tracing::Span::current().record("participation.status", &participation.status);
    tracing::info!(
        event = "event.join_request_reviewed",
        event_id,
        applicant_id,
        reviewer_id = auth.user_id,
        participation_status = %participation.status,
        "event join request reviewed"
    );
    Ok(Json(to_event_participation_resp(participation)?))
}

async fn ensure_event_capacity_available(
    event: &Event,
    conn: &mut DieselConn,
) -> Result<(), HttpErr> {
    let Some(capacity) = event.capacity else {
        return Ok(());
    };
    let joined_count = EventParticipation::count_joined(event.event_id, conn)
        .await
        .internal(DB_ERROR)?;
    if joined_count >= i64::from(capacity) {
        tracing::warn!(
            event = "event.capacity_reached",
            event_id = event.event_id,
            capacity,
            joined_count,
            "event join rejected because capacity was reached"
        );
        return Err(HttpErr::client(
            StatusCode::CONFLICT,
            EVENT_CAPACITY_REACHED,
        ));
    }
    Ok(())
}

fn to_event_participation_resp(
    participation: EventParticipation,
) -> Result<EventParticipationResp, HttpErr> {
    Ok(EventParticipationResp {
        event_id: participation.event_id.to_string(),
        user_id: participation.user_id.to_string(),
        status: to_event_participation_status(&participation.status)?,
        requested_at: participation.requested_at.to_rfc3339(),
        reviewed_at: participation.reviewed_at.map(|value| value.to_rfc3339()),
        joined_at: participation.joined_at.map(|value| value.to_rfc3339()),
    })
}

fn to_event_participation_status(value: &str) -> Result<EventParticipationStatus, HttpErr> {
    match value {
        "pending" => Ok(EventParticipationStatus::Pending),
        "joined" => Ok(EventParticipationStatus::Joined),
        "rejected" => Ok(EventParticipationStatus::Rejected),
        _ => Err(HttpErr::internal(
            DB_ERROR,
            anyhow::anyhow!("unknown event participation status: {value}"),
        )),
    }
}

fn participation_status_name(value: &EventParticipationStatus) -> &'static str {
    match value {
        EventParticipationStatus::Pending => "pending",
        EventParticipationStatus::Joined => "joined",
        EventParticipationStatus::Rejected => "rejected",
    }
}

fn to_event_resp(event: Event) -> Result<EventResp, HttpErr> {
    let content = parse_stored_content(&event)?;
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
        content_version: event.content_version,
        content,
        summary: event.summary,
        cover_asset_id: event.cover_asset_id,
        start_at: event.start_at.map(|value| value.to_rfc3339()),
        end_at: event.end_at.map(|value| value.to_rfc3339()),
        location_name: event.location_name,
        location_address: event.location_address,
        capacity: event.capacity,
        requires_approval: event.requires_approval,
        tags: event.tags,
        created_at: event.created_at.to_rfc3339(),
        updated_at: event.updated_at.to_rfc3339(),
    })
}

async fn insert_event(
    creator_id: i64,
    status: &str,
    draft: NormalizedEventDraft,
    published_at: Option<DateTime<Utc>>,
) -> Result<Event, HttpErr> {
    let mut conn = db_conn!();
    insert_event_with_conn(creator_id, status, draft, published_at, &mut conn).await
}

async fn insert_event_with_conn(
    creator_id: i64,
    status: &str,
    draft: NormalizedEventDraft,
    published_at: Option<DateTime<Utc>>,
    conn: &mut DieselConn,
) -> Result<Event, HttpErr> {
    let event = NewEvent {
        creator_id,
        title: draft.title,
        status: status.to_string(),
        content_version: EVENT_CONTENT_VERSION_V1,
        content_doc: serde_json::to_value(&draft.content).internal(DB_ERROR)?,
        summary: build_summary(&draft.content_stats.plain_text),
        cover_asset_id: cover_asset_id(&draft.content),
        start_at: draft.start_at,
        end_at: draft.end_at,
        location_name: draft.location_name,
        location_address: draft.location_address,
        capacity: draft.capacity,
        requires_approval: draft.requires_approval,
        tags: draft.tags,
        published_at,
    };

    Event::insert(event, conn).await.internal(DB_ERROR)
}

async fn publish_locked_event_draft(
    user_id: i64,
    event: Event,
    conn: &mut DieselConn,
) -> Result<Event, HttpErr> {
    let event_id = event.event_id;
    let asset_ids = collect_image_asset_ids(&parse_stored_content(&event)?);
    validate_referenced_media_assets_with_conn(user_id, &asset_ids, conn).await?;
    let publish_changes = build_publish_changes(event)?;
    Event::publish_draft_with_changes(event_id, user_id, publish_changes, conn)
        .await
        .map_err(map_event_mutation_error)
}

fn normalize_publish_payload(req: CreateEventRequest) -> Result<NormalizedEventDraft, HttpErr> {
    normalize_full_payload(RawEventPayload {
        title: req.title,
        content: req.content,
        start_at: req.start_at,
        end_at: req.end_at,
        location_name: req.location_name,
        location_address: req.location_address,
        capacity: req.capacity,
        requires_approval: req.requires_approval,
        tags: req.tags,
        mode: ContentValidationMode::Publish,
    })
}

fn normalize_draft_update_payload(
    req: UpdateEventDraftRequest,
) -> Result<NormalizedEventDraft, HttpErr> {
    normalize_full_payload(RawEventPayload {
        title: req.title,
        content: req.content,
        start_at: req.start_at,
        end_at: req.end_at,
        location_name: req.location_name,
        location_address: req.location_address,
        capacity: req.capacity,
        requires_approval: req.requires_approval,
        tags: req.tags,
        mode: ContentValidationMode::Draft,
    })
}

fn normalize_draft_create_payload(
    req: CreateEventDraftRequest,
) -> Result<NormalizedEventDraft, HttpErr> {
    normalize_full_payload(RawEventPayload {
        title: req.title.unwrap_or_default(),
        content: req.content.unwrap_or_else(empty_content_doc),
        start_at: req.start_at,
        end_at: req.end_at,
        location_name: req.location_name,
        location_address: req.location_address,
        capacity: req.capacity,
        requires_approval: req.requires_approval,
        tags: req.tags.unwrap_or_default(),
        mode: ContentValidationMode::Draft,
    })
}

fn normalize_full_payload(raw: RawEventPayload) -> Result<NormalizedEventDraft, HttpErr> {
    let RawEventPayload {
        title,
        content,
        start_at,
        end_at,
        location_name,
        location_address,
        capacity,
        requires_approval,
        tags,
        mode,
    } = raw;
    let title = match mode {
        ContentValidationMode::Draft => normalize_draft_title(title)?,
        ContentValidationMode::Publish => normalize_required_text(&title, MAX_TITLE_CHARS)?,
    };
    validate_content_version(&content)?;
    let content_stats = match mode {
        ContentValidationMode::Draft => validate_content_for_draft(&content)?,
        ContentValidationMode::Publish => validate_content_for_publish(&content)?,
    };
    let start_at = parse_optional_time(start_at.as_deref())?;
    let end_at = parse_optional_time(end_at.as_deref())?;
    validate_time_range(start_at, end_at)?;

    Ok(NormalizedEventDraft {
        asset_ids: collect_image_asset_ids(&content),
        content,
        content_stats,
        title,
        tags: normalize_tags(tags)?,
        start_at,
        end_at,
        location_name: normalize_optional_text(location_name, 80)?,
        location_address: normalize_optional_text(location_address, 200)?,
        capacity: validate_capacity(capacity)?,
        requires_approval,
    })
}

fn to_draft_changes(draft: NormalizedEventDraft) -> Result<EventDraftChanges, HttpErr> {
    Ok(EventDraftChanges {
        title: draft.title,
        content_version: EVENT_CONTENT_VERSION_V1,
        content_doc: serde_json::to_value(&draft.content).internal(DB_ERROR)?,
        summary: build_summary(&draft.content_stats.plain_text),
        cover_asset_id: cover_asset_id(&draft.content),
        start_at: draft.start_at,
        end_at: draft.end_at,
        location_name: draft.location_name,
        location_address: draft.location_address,
        capacity: draft.capacity,
        requires_approval: draft.requires_approval,
        tags: draft.tags,
    })
}

fn build_publish_changes(event: Event) -> Result<PublishEventDraftChanges, HttpErr> {
    let content = parse_stored_content(&event)?;
    let title = normalize_required_text(&event.title, MAX_TITLE_CHARS)?;
    let content_stats = validate_content_for_publish(&content)?;
    let start_at = event.start_at;
    let end_at = event.end_at;
    validate_time_range(start_at, end_at)?;

    Ok(PublishEventDraftChanges {
        title,
        content_version: EVENT_CONTENT_VERSION_V1,
        content_doc: serde_json::to_value(&content).internal(DB_ERROR)?,
        summary: build_summary(&content_stats.plain_text),
        cover_asset_id: cover_asset_id(&content),
        start_at,
        end_at,
        location_name: event.location_name,
        location_address: event.location_address,
        capacity: validate_capacity(event.capacity)?,
        requires_approval: event.requires_approval,
        tags: normalize_tags(event.tags)?,
        status: "published".to_string(),
        published_at: Some(Utc::now()),
    })
}

#[derive(Clone, Copy)]
enum ContentValidationMode {
    Draft,
    Publish,
}

fn normalize_draft_title(value: String) -> Result<String, HttpErr> {
    let normalized = value.trim().to_string();
    if normalized.chars().count() > MAX_TITLE_CHARS {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    Ok(normalized)
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

fn validate_time_range(
    start_at: Option<DateTime<Utc>>,
    end_at: Option<DateTime<Utc>>,
) -> Result<(), HttpErr> {
    if start_at
        .zip(end_at)
        .is_some_and(|(start, end)| end <= start)
    {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    Ok(())
}

fn parse_stored_content(event: &Event) -> Result<EventContentDoc, HttpErr> {
    let content =
        serde_json::from_value::<EventContentDoc>(event.content_doc.clone()).internal(DB_ERROR)?;
    validate_content_version(&content)?;
    Ok(content)
}

fn validate_content_version(content: &EventContentDoc) -> Result<(), HttpErr> {
    if content.version != EVENT_CONTENT_VERSION_V1 {
        tracing::warn!(
            event = "event.content_version.unsupported",
            content_version = content.version,
            supported_version = EVENT_CONTENT_VERSION_V1,
            "unsupported event content version"
        );
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    Ok(())
}

fn empty_content_doc() -> EventContentDoc {
    EventContentDoc {
        version: EVENT_CONTENT_VERSION_V1,
        blocks: Vec::new(),
    }
}

fn validate_content_for_draft(content: &EventContentDoc) -> Result<ContentStats, HttpErr> {
    if content.blocks.len() > MAX_BLOCKS {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    validate_content_blocks(content, false)
}

fn validate_content_for_publish(content: &EventContentDoc) -> Result<ContentStats, HttpErr> {
    if content.blocks.is_empty() || content.blocks.len() > MAX_BLOCKS {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }

    let stats = validate_content_blocks(content, true)?;
    if stats.text_chars == 0 && stats.image_count == 0 {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    Ok(stats)
}

fn validate_content_blocks(
    content: &EventContentDoc,
    require_complete_text_blocks: bool,
) -> Result<ContentStats, HttpErr> {
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
                validate_text_children(children, &mut stats, require_complete_text_blocks)?;
            }
            EventContentBlock::Paragraph { id, children } => {
                validate_block_id(id, &mut block_ids)?;
                validate_text_children(children, &mut stats, require_complete_text_blocks)?;
            }
            EventContentBlock::Quote { id, children } => {
                validate_block_id(id, &mut block_ids)?;
                validate_text_children(children, &mut stats, require_complete_text_blocks)?;
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
    require_complete_text_blocks: bool,
) -> Result<(), HttpErr> {
    if children.len() > MAX_INLINE_NODES_PER_PARAGRAPH {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    if require_complete_text_blocks && children.is_empty() {
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

#[tracing::instrument(
    name = "event.media_assets.validate",
    skip_all,
    fields(user.id = owner_id, media.asset_count = asset_ids.len())
)]
async fn validate_referenced_media_assets(
    owner_id: i64,
    asset_ids: &HashSet<String>,
) -> Result<(), HttpErr> {
    validate_referenced_media_assets_with_conn(owner_id, asset_ids, &mut db_conn!()).await
}

async fn validate_referenced_media_assets_with_conn(
    owner_id: i64,
    asset_ids: &HashSet<String>,
    conn: &mut loop_svc_model::DieselConn,
) -> Result<(), HttpErr> {
    if asset_ids.is_empty() {
        return Ok(());
    }

    let ids = asset_ids.iter().cloned().collect::<Vec<_>>();
    let rows = MediaAsset::select_uploaded_by_ids_for_owner(&ids, owner_id, conn)
        .await
        .internal(DB_ERROR)?;
    if rows.len() != ids.len() {
        tracing::warn!(
            event = "event.media_assets.invalid",
            user_id = owner_id,
            requested_asset_count = ids.len(),
            valid_asset_count = rows.len(),
            "event references media assets that are missing, not uploaded, or owned by another user"
        );
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    Ok(())
}

fn collect_image_asset_ids(content: &EventContentDoc) -> HashSet<String> {
    let mut asset_ids = HashSet::new();
    for block in &content.blocks {
        match block {
            EventContentBlock::Image { item, .. } => {
                asset_ids.insert(item.asset_id.trim().to_string());
            }
            EventContentBlock::ImageGrid { items, .. } => {
                asset_ids.extend(items.iter().map(|item| item.asset_id.trim().to_string()));
            }
            EventContentBlock::Divider { .. }
            | EventContentBlock::Feature { .. }
            | EventContentBlock::Heading { .. }
            | EventContentBlock::Paragraph { .. }
            | EventContentBlock::Quote { .. } => {}
        }
    }
    asset_ids
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

fn parse_event_id(value: &str) -> Result<i64, HttpErr> {
    value
        .parse::<i64>()
        .map_err(|_| HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT))
}

fn parse_user_id(value: &str) -> Result<i64, HttpErr> {
    value
        .parse::<i64>()
        .map_err(|_| HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT))
}

fn parse_optional_status(value: Option<&str>) -> Result<Option<String>, HttpErr> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let normalized = value.to_ascii_lowercase();
    match normalized.as_str() {
        "draft" | "published" | "cancelled" => Ok(Some(normalized)),
        _ => Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT)),
    }
}

fn map_event_mutation_error(err: diesel::result::Error) -> HttpErr {
    match err {
        diesel::result::Error::NotFound => HttpErr::client(StatusCode::NOT_FOUND, EVENT_NOT_FOUND),
        err => HttpErr::internal(DB_ERROR, err),
    }
}

pub fn route(state: crate::http::AppState) -> AppRoutes {
    AppRoutes::<crate::http::AppState>::new()
        .public(Method::GET, "/event", get(list_events))
        .public(Method::GET, "/event/{event_id}", get(get_event))
        .protected(
            Method::GET,
            "/me/events",
            EVENT_CREATE_PERMISSION,
            get(list_owned_events),
        )
        .protected(
            Method::GET,
            "/me/joined-events",
            EVENT_JOIN_PERMISSION,
            get(list_joined_events),
        )
        .protected(
            Method::POST,
            "/event",
            EVENT_CREATE_PERMISSION,
            post(create_event),
        )
        .protected(
            Method::POST,
            "/event/drafts",
            EVENT_CREATE_PERMISSION,
            post(create_event_draft),
        )
        .protected(
            Method::PATCH,
            "/event/{event_id}",
            EVENT_CREATE_PERMISSION,
            patch(update_event_draft),
        )
        .protected(
            Method::DELETE,
            "/event/{event_id}",
            EVENT_CREATE_PERMISSION,
            delete(delete_event_draft),
        )
        .protected(
            Method::POST,
            "/event/{event_id}/publish",
            EVENT_CREATE_PERMISSION,
            post(publish_event_draft),
        )
        .protected(
            Method::GET,
            "/event/{event_id}/participation",
            EVENT_JOIN_PERMISSION,
            get(get_event_participation),
        )
        .protected(
            Method::POST,
            "/event/{event_id}/join",
            EVENT_JOIN_PERMISSION,
            post(join_event),
        )
        .protected(
            Method::GET,
            "/event/{event_id}/join-requests",
            EVENT_CREATE_PERMISSION,
            get(list_event_join_requests),
        )
        .protected(
            Method::PATCH,
            "/event/{event_id}/join-requests/{applicant_id}",
            EVENT_CREATE_PERMISSION,
            patch(review_event_join_request),
        )
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_doc(text: &str) -> EventContentDoc {
        EventContentDoc {
            version: EVENT_CONTENT_VERSION_V1,
            blocks: vec![EventContentBlock::Paragraph {
                id: "p1".to_string(),
                children: vec![EventInlineNode::Text {
                    text: text.to_string(),
                    marks: Vec::new(),
                }],
            }],
        }
    }

    fn draft_event(content: EventContentDoc) -> Event {
        Event {
            event_id: 1,
            creator_id: 7,
            title: "  活动标题  ".to_string(),
            status: "draft".to_string(),
            content_version: EVENT_CONTENT_VERSION_V1,
            content_doc: serde_json::to_value(content).expect("content should serialize"),
            summary: String::new(),
            cover_asset_id: None,
            start_at: None,
            end_at: None,
            location_name: Some("  场地  ".to_string()),
            location_address: None,
            capacity: Some(20),
            requires_approval: true,
            tags: vec![
                " rust ".to_string(),
                "#rust".to_string(),
                "后端".to_string(),
            ],
            created_at: Utc::now(),
            updated_at: Utc::now(),
            published_at: None,
        }
    }

    #[test]
    fn validate_content_accepts_text_doc_for_publish() {
        let stats =
            validate_content_for_publish(&text_doc("活动介绍")).expect("content should be valid");
        assert_eq!(stats.text_chars, 4);
        assert_eq!(stats.image_count, 0);
    }

    #[test]
    fn validate_content_rejects_empty_text_for_publish() {
        validate_content_for_publish(&text_doc("   "))
            .expect_err("empty content should be rejected");
    }

    #[test]
    fn validate_content_allows_empty_draft() {
        let stats =
            validate_content_for_draft(&empty_content_doc()).expect("empty draft should be valid");
        assert_eq!(stats.text_chars, 0);
        assert_eq!(stats.image_count, 0);
    }

    #[test]
    fn parse_optional_status_rejects_unknown_status() {
        parse_optional_status(Some("archived")).expect_err("unknown status should be rejected");
    }

    #[test]
    fn build_publish_changes_normalizes_publish_snapshot() {
        let changes =
            build_publish_changes(draft_event(text_doc("活动介绍"))).expect("draft should publish");

        assert_eq!(changes.title, "活动标题");
        assert_eq!(changes.summary, "活动介绍");
        assert_eq!(changes.status, "published");
        assert!(changes.published_at.is_some());
        assert_eq!(changes.capacity, Some(20));
        assert!(changes.requires_approval);
        assert_eq!(changes.tags, vec!["rust".to_string(), "后端".to_string()]);
    }

    #[test]
    fn build_publish_changes_rejects_empty_draft_content() {
        build_publish_changes(draft_event(empty_content_doc()))
            .expect_err("empty draft should not publish");
    }
}
