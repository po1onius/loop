use super::validation::{
    normalize_asset_ids, normalize_limit, parse_uuid, validate_image_assets_with_conn,
};
use crate::{
    db_conn,
    http::{AppRoutes, AppState, AuthInfo, HttpErr, OptionExt, ResultExt, err_key::*},
};
use axum::{
    Json,
    extract::{Path, Query, State},
    routing::{delete, get, patch, post, put},
};
use chrono::{DateTime, TimeZone, Utc};
use diesel_async::AsyncConnection;
use http::{Method, StatusCode};
use loop_dto::{
    CommunityPostReactionResp, CommunityPostResp, CommunityPostType, CommunitySectionResp,
    CreateCommunityPostRequest, ListCommunityPostsResp, UpdateCommunityPostRequest,
};
use loop_svc_model::{
    community::{
        CommunityPost, CommunityPostChanges, CommunityPostRecord, CommunitySection,
        NewCommunityPost, NewCommunityPostEventLink, add_interested_reaction,
        delete_post_event_links, insert_post_event_link, remove_interested_reaction,
        select_interested_post_ids,
    },
    conversation::{Conversation, ConversationReadState, NewConversation},
    event::Event,
};
use serde::Deserialize;
use std::collections::HashSet;
use uuid::Uuid;

const COMMUNITY_READ_PERMISSION: &str = "community.read";
const COMMUNITY_POST_CREATE_PERMISSION: &str = "community.post.create";
const DEFAULT_POST_LIMIT: i64 = 20;
const MAX_POST_LIMIT: i64 = 50;
const MAX_POST_TITLE_CHARS: usize = 120;
const MAX_POST_BODY_CHARS: usize = 10_000;
const MAX_POST_IMAGES: usize = 9;

#[derive(Debug, Deserialize)]
pub struct ListPostsQuery {
    section_id: Option<String>,
    sort: Option<String>,
    cursor: Option<String>,
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ListMyPostsQuery {
    cursor: Option<String>,
    limit: Option<i64>,
}

struct NormalizedPostPayload {
    section_id: String,
    post_type: &'static str,
    title: String,
    body: String,
    image_asset_ids: Vec<String>,
    discussed_event_id: Option<i64>,
}

#[tracing::instrument(name = "community.section.list", skip_all, fields(user.id = auth.user_id))]
pub async fn list_sections(auth: AuthInfo) -> Result<Json<Vec<CommunitySectionResp>>, HttpErr> {
    let sections = CommunitySection::list_active(&mut db_conn!())
        .await
        .internal(DB_ERROR)?;
    tracing::info!(
        event = "community.sections.loaded",
        user_id = auth.user_id,
        section_count = sections.len(),
        "community sections loaded"
    );
    Ok(Json(
        sections
            .into_iter()
            .map(|section| CommunitySectionResp {
                section_id: section.section_id,
                name: section.name,
                description: section.description,
            })
            .collect(),
    ))
}

#[tracing::instrument(name = "community.post.list", skip_all, fields(user.id = auth.user_id))]
pub async fn list_posts(
    auth: AuthInfo,
    Query(query): Query<ListPostsQuery>,
) -> Result<Json<ListCommunityPostsResp>, HttpErr> {
    list_posts_for_user(
        auth.user_id,
        query.section_id.as_deref(),
        query.sort.as_deref(),
        query.cursor.as_deref(),
        query.limit,
        None,
    )
    .await
}

#[tracing::instrument(name = "community.post.mine", skip_all, fields(user.id = auth.user_id))]
pub async fn list_my_posts(
    auth: AuthInfo,
    Query(query): Query<ListMyPostsQuery>,
) -> Result<Json<ListCommunityPostsResp>, HttpErr> {
    list_posts_for_user(
        auth.user_id,
        None,
        Some("latest"),
        query.cursor.as_deref(),
        query.limit,
        Some(auth.user_id),
    )
    .await
}

async fn list_posts_for_user(
    viewer_id: i64,
    section_id: Option<&str>,
    sort: Option<&str>,
    cursor: Option<&str>,
    limit: Option<i64>,
    author_id: Option<i64>,
) -> Result<Json<ListCommunityPostsResp>, HttpErr> {
    let sort_by_activity = match sort.unwrap_or("latest") {
        "latest" => false,
        "active" => true,
        _ => return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT)),
    };
    let limit = normalize_limit(limit, DEFAULT_POST_LIMIT, MAX_POST_LIMIT)?;
    let (cursor_at, cursor_id) = parse_post_cursor(cursor)?;
    let mut conn = db_conn!();
    let mut records = CommunityPost::list_records(
        section_id,
        author_id,
        sort_by_activity,
        cursor_at,
        cursor_id,
        limit + 1,
        &mut conn,
    )
    .await
    .internal(DB_ERROR)?;
    let has_more = records.len() as i64 > limit;
    if has_more {
        records.pop();
    }
    let post_ids = records
        .iter()
        .map(|record| record.post.post_id)
        .collect::<Vec<_>>();
    let interested = select_interested_post_ids(viewer_id, &post_ids, &mut conn)
        .await
        .internal(DB_ERROR)?
        .into_iter()
        .collect::<HashSet<_>>();
    let next_cursor = has_more
        .then(|| records.last())
        .flatten()
        .map(|record| encode_post_cursor(record, sort_by_activity));
    tracing::info!(
        event = "community.posts.loaded",
        user_id = viewer_id,
        item_count = records.len(),
        sort_by_activity,
        has_more,
        "community posts loaded"
    );
    Ok(Json(ListCommunityPostsResp {
        items: records
            .into_iter()
            .map(|record| {
                let viewer_interested = interested.contains(&record.post.post_id);
                to_post_resp(record, viewer_interested)
            })
            .collect::<Result<Vec<_>, _>>()?,
        next_cursor,
    }))
}

#[tracing::instrument(name = "community.post.get", skip_all, fields(user.id = auth.user_id, community.post.id = %post_id))]
pub async fn get_post(
    auth: AuthInfo,
    Path(post_id): Path<String>,
) -> Result<Json<CommunityPostResp>, HttpErr> {
    let post_id = parse_uuid(&post_id)?;
    let mut conn = db_conn!();
    let record = CommunityPost::select_record(post_id, &mut conn)
        .await
        .internal(DB_ERROR)?
        .client(StatusCode::NOT_FOUND, COMMUNITY_POST_NOT_FOUND)?;
    let interested = !select_interested_post_ids(auth.user_id, &[post_id], &mut conn)
        .await
        .internal(DB_ERROR)?
        .is_empty();
    Ok(Json(to_post_resp(record, interested)?))
}

#[tracing::instrument(name = "community.post.create", skip_all, fields(user.id = auth.user_id, community.post.id = tracing::field::Empty))]
pub async fn create_post(
    State(_state): State<AppState>,
    auth: AuthInfo,
    Json(req): Json<CreateCommunityPostRequest>,
) -> Result<(StatusCode, Json<CommunityPostResp>), HttpErr> {
    let payload = normalize_create_payload(req)?;
    validate_post_references(auth.user_id, &payload).await?;
    let post_id = Uuid::now_v7();
    let conversation_id = Uuid::now_v7();
    let subject_id = post_id.to_string();
    let mut conn = db_conn!();
    conn.transaction::<(), HttpErr, _>(async |conn| {
        Conversation::insert(
            NewConversation {
                conversation_id,
                kind: "post_thread",
                subject_id: &subject_id,
                access_mode: "open",
                status: "active",
                title: &payload.title,
            },
            conn,
        )
        .await
        .internal(DB_ERROR)?;
        CommunityPost::insert(
            NewCommunityPost {
                post_id,
                author_id: auth.user_id,
                section_id: &payload.section_id,
                post_type: payload.post_type,
                title: &payload.title,
                body: &payload.body,
                image_asset_ids: &payload.image_asset_ids,
                status: "published",
                discussion_conversation_id: conversation_id,
            },
            conn,
        )
        .await
        .internal(DB_ERROR)?;
        ConversationReadState::set_subscription(conversation_id, auth.user_id, true, false, conn)
            .await
            .internal(DB_ERROR)?;
        if let Some(event_id) = payload.discussed_event_id {
            insert_post_event_link(
                NewCommunityPostEventLink {
                    post_id,
                    event_id,
                    relation_type: "discusses",
                    created_by: auth.user_id,
                },
                conn,
            )
            .await
            .internal(DB_ERROR)?;
        }
        Ok(())
    })
    .await?;
    let record = CommunityPost::select_record(post_id, &mut conn)
        .await
        .internal(DB_ERROR)?
        .client(StatusCode::INTERNAL_SERVER_ERROR, DB_ERROR)?;
    tracing::Span::current().record("community.post.id", tracing::field::display(post_id));
    tracing::info!(
        event = "community.post.created",
        user_id = auth.user_id,
        %post_id,
        %conversation_id,
        post_type = payload.post_type,
        image_count = payload.image_asset_ids.len(),
        "community post and discussion conversation created"
    );
    Ok((StatusCode::CREATED, Json(to_post_resp(record, false)?)))
}

#[tracing::instrument(name = "community.post.update", skip_all, fields(user.id = auth.user_id, community.post.id = %post_id))]
pub async fn update_post(
    auth: AuthInfo,
    Path(post_id): Path<String>,
    Json(req): Json<UpdateCommunityPostRequest>,
) -> Result<Json<CommunityPostResp>, HttpErr> {
    let post_id = parse_uuid(&post_id)?;
    let payload = normalize_update_payload(req)?;
    validate_post_references(auth.user_id, &payload).await?;
    let now = Utc::now();
    let mut conn = db_conn!();
    let post = conn
        .transaction::<CommunityPost, HttpErr, _>(async |conn| {
            let existing = CommunityPost::select_record(post_id, conn)
                .await
                .internal(DB_ERROR)?
                .client(StatusCode::NOT_FOUND, COMMUNITY_POST_NOT_FOUND)?;
            if existing.post.author_id != auth.user_id {
                return Err(HttpErr::client(StatusCode::FORBIDDEN, PERMISSION_DENIED));
            }
            // Use the same conversation -> post lock order as message creation.
            Conversation::lock_for_send(existing.post.discussion_conversation_id, conn)
                .await
                .internal(DB_ERROR)?;
            let post = CommunityPost::update_owned(
                post_id,
                auth.user_id,
                CommunityPostChanges {
                    section_id: &payload.section_id,
                    post_type: payload.post_type,
                    title: &payload.title,
                    body: &payload.body,
                    image_asset_ids: &payload.image_asset_ids,
                    updated_at: now,
                    edited_at: Some(now),
                },
                conn,
            )
            .await
            .internal(DB_ERROR)?
            .client(StatusCode::NOT_FOUND, COMMUNITY_POST_NOT_FOUND)?;
            Conversation::update_title(post.discussion_conversation_id, &payload.title, conn)
                .await
                .internal(DB_ERROR)?;
            // 活动关联和帖子内容一起提交，避免页面已显示“活动见闻”但关联记录仍
            // 指向旧活动。数据库不使用外键，完整性由这段事务显式保证。
            delete_post_event_links(post_id, "discusses", conn)
                .await
                .internal(DB_ERROR)?;
            if let Some(event_id) = payload.discussed_event_id {
                insert_post_event_link(
                    NewCommunityPostEventLink {
                        post_id,
                        event_id,
                        relation_type: "discusses",
                        created_by: auth.user_id,
                    },
                    conn,
                )
                .await
                .internal(DB_ERROR)?;
            }
            Ok(post)
        })
        .await?;
    let record = CommunityPost::select_record(post.post_id, &mut conn)
        .await
        .internal(DB_ERROR)?
        .client(StatusCode::NOT_FOUND, COMMUNITY_POST_NOT_FOUND)?;
    let interested = !select_interested_post_ids(auth.user_id, &[post_id], &mut conn)
        .await
        .internal(DB_ERROR)?
        .is_empty();
    tracing::info!(event = "community.post.updated", user_id = auth.user_id, %post_id, "community post updated");
    Ok(Json(to_post_resp(record, interested)?))
}

#[tracing::instrument(name = "community.post.delete", skip_all, fields(user.id = auth.user_id, community.post.id = %post_id))]
pub async fn delete_post(
    auth: AuthInfo,
    Path(post_id): Path<String>,
) -> Result<StatusCode, HttpErr> {
    let post_id = parse_uuid(&post_id)?;
    let mut conn = db_conn!();
    conn.transaction::<(), HttpErr, _>(async |conn| {
        let record = CommunityPost::select_record(post_id, conn)
            .await
            .internal(DB_ERROR)?
            .client(StatusCode::NOT_FOUND, COMMUNITY_POST_NOT_FOUND)?;
        if record.post.author_id != auth.user_id {
            return Err(HttpErr::client(StatusCode::FORBIDDEN, PERMISSION_DENIED));
        }
        Conversation::lock(record.post.discussion_conversation_id, conn)
            .await
            .internal(DB_ERROR)?;
        CommunityPost::soft_delete_owned(post_id, auth.user_id, conn)
            .await
            .internal(DB_ERROR)?;
        Ok(())
    })
    .await?;
    tracing::info!(event = "community.post.deleted", user_id = auth.user_id, %post_id, "community post soft deleted and discussion locked");
    Ok(StatusCode::NO_CONTENT)
}

#[tracing::instrument(name = "community.post.interested.add", skip_all, fields(user.id = auth.user_id, community.post.id = %post_id))]
pub async fn add_interested(
    auth: AuthInfo,
    Path(post_id): Path<String>,
) -> Result<Json<CommunityPostReactionResp>, HttpErr> {
    set_interested(auth.user_id, &post_id, true).await
}

#[tracing::instrument(name = "community.post.interested.remove", skip_all, fields(user.id = auth.user_id, community.post.id = %post_id))]
pub async fn remove_interested(
    auth: AuthInfo,
    Path(post_id): Path<String>,
) -> Result<Json<CommunityPostReactionResp>, HttpErr> {
    set_interested(auth.user_id, &post_id, false).await
}

async fn set_interested(
    user_id: i64,
    post_id: &str,
    interested: bool,
) -> Result<Json<CommunityPostReactionResp>, HttpErr> {
    let post_id = parse_uuid(post_id)?;
    let mut conn = db_conn!();
    // 反应记录与帖子聚合计数必须在同一事务内变化。否则并发请求或任意一条
    // SQL 失败时会出现“用户已经点过，但列表计数没有同步”的永久脏数据。
    let interest_count = conn
        .transaction::<i64, HttpErr, _>(async |conn| {
            CommunityPost::select_record(post_id, conn)
                .await
                .internal(DB_ERROR)?
                .client(StatusCode::NOT_FOUND, COMMUNITY_POST_NOT_FOUND)?;
            if interested {
                add_interested_reaction(post_id, user_id, conn)
                    .await
                    .internal(DB_ERROR)
            } else {
                remove_interested_reaction(post_id, user_id, conn)
                    .await
                    .internal(DB_ERROR)
            }
        })
        .await?;
    tracing::info!(event = "community.post.interested.changed", user_id, %post_id, interested, interest_count, "community post interested reaction changed");
    Ok(Json(CommunityPostReactionResp {
        interested,
        interest_count,
    }))
}

fn normalize_create_payload(
    req: CreateCommunityPostRequest,
) -> Result<NormalizedPostPayload, HttpErr> {
    let discussed_event_id = req
        .discussed_event_id
        .as_deref()
        .map(parse_i64)
        .transpose()?;
    normalize_post_payload(
        req.section_id,
        req.post_type,
        req.title,
        req.body,
        req.image_asset_ids,
        discussed_event_id,
    )
}

fn normalize_update_payload(
    req: UpdateCommunityPostRequest,
) -> Result<NormalizedPostPayload, HttpErr> {
    let discussed_event_id = req
        .discussed_event_id
        .as_deref()
        .map(parse_i64)
        .transpose()?;
    normalize_post_payload(
        req.section_id,
        req.post_type,
        req.title,
        req.body,
        req.image_asset_ids,
        discussed_event_id,
    )
}

fn normalize_post_payload(
    section_id: String,
    post_type: CommunityPostType,
    title: String,
    body: String,
    image_asset_ids: Vec<String>,
    discussed_event_id: Option<i64>,
) -> Result<NormalizedPostPayload, HttpErr> {
    let section_id = section_id.trim().to_ascii_lowercase();
    let title = title.trim().to_string();
    let body = body.trim().to_string();
    if section_id.is_empty()
        || title.is_empty()
        || title.chars().count() > MAX_POST_TITLE_CHARS
        || body.is_empty()
        || body.chars().count() > MAX_POST_BODY_CHARS
    {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    let post_type = match post_type {
        CommunityPostType::EventIdea => "event_idea",
        CommunityPostType::EventDiscussion => "event_discussion",
        CommunityPostType::General => "general",
    };
    if post_type == "event_discussion" && discussed_event_id.is_none() {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    Ok(NormalizedPostPayload {
        section_id,
        post_type,
        title,
        body,
        image_asset_ids: normalize_asset_ids(image_asset_ids, MAX_POST_IMAGES)?,
        discussed_event_id,
    })
}

async fn validate_post_references(
    user_id: i64,
    payload: &NormalizedPostPayload,
) -> Result<(), HttpErr> {
    let mut conn = db_conn!();
    CommunitySection::exists_active(&payload.section_id, &mut conn)
        .await
        .internal(DB_ERROR)?
        .then_some(())
        .client(StatusCode::BAD_REQUEST, COMMUNITY_SECTION_NOT_FOUND)?;
    validate_image_assets_with_conn(user_id, &payload.image_asset_ids, &mut conn).await?;
    if let Some(event_id) = payload.discussed_event_id {
        Event::select_published_by_id(event_id, &mut conn)
            .await
            .internal(DB_ERROR)?
            .client(StatusCode::BAD_REQUEST, EVENT_NOT_FOUND)?;
    }
    Ok(())
}

fn to_post_resp(
    record: CommunityPostRecord,
    viewer_interested: bool,
) -> Result<CommunityPostResp, HttpErr> {
    let post_type = match record.post.post_type.as_str() {
        "event_idea" => CommunityPostType::EventIdea,
        "event_discussion" => CommunityPostType::EventDiscussion,
        "general" => CommunityPostType::General,
        _ => return Err(HttpErr::client(StatusCode::INTERNAL_SERVER_ERROR, DB_ERROR)),
    };
    Ok(CommunityPostResp {
        post_id: record.post.post_id.to_string(),
        author_id: record.post.author_id.to_string(),
        author_username: record.author_username,
        author_avatar_asset_id: record.author_avatar_asset_id,
        section_id: record.post.section_id,
        section_name: record.section_name,
        post_type,
        title: record.post.title,
        body: record.post.body,
        image_asset_ids: record.post.image_asset_ids,
        discussion_conversation_id: record.post.discussion_conversation_id.to_string(),
        discussion_count: record.post.discussion_count,
        interest_count: record.post.interest_count,
        viewer_interested,
        created_at: record.post.created_at.to_rfc3339(),
        updated_at: record.post.updated_at.to_rfc3339(),
        last_activity_at: record.post.last_activity_at.to_rfc3339(),
        edited_at: record.post.edited_at.map(|value| value.to_rfc3339()),
    })
}

fn parse_post_cursor(
    value: Option<&str>,
) -> Result<(Option<DateTime<Utc>>, Option<Uuid>), HttpErr> {
    let Some(value) = value else {
        return Ok((None, None));
    };
    let (timestamp, post_id) = value
        .split_once(':')
        .ok_or_else(|| HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT))?;
    let timestamp = timestamp
        .parse::<i64>()
        .ok()
        .and_then(|value| Utc.timestamp_millis_opt(value).single())
        .ok_or_else(|| HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT))?;
    Ok((Some(timestamp), Some(parse_uuid(post_id)?)))
}

fn encode_post_cursor(record: &CommunityPostRecord, sort_by_activity: bool) -> String {
    let timestamp = if sort_by_activity {
        record.post.last_activity_at
    } else {
        record.post.created_at
    };
    format!("{}:{}", timestamp.timestamp_millis(), record.post.post_id)
}

fn parse_i64(value: &str) -> Result<i64, HttpErr> {
    value
        .trim()
        .parse::<i64>()
        .map_err(|_| HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT))
}

pub fn route(state: AppState) -> AppRoutes {
    AppRoutes::<AppState>::new()
        .protected(
            Method::GET,
            "/community/sections",
            COMMUNITY_READ_PERMISSION,
            get(list_sections),
        )
        .protected(
            Method::GET,
            "/community/posts",
            COMMUNITY_READ_PERMISSION,
            get(list_posts),
        )
        .protected(
            Method::GET,
            "/community/posts/{post_id}",
            COMMUNITY_READ_PERMISSION,
            get(get_post),
        )
        .protected(
            Method::GET,
            "/me/community/posts",
            COMMUNITY_READ_PERMISSION,
            get(list_my_posts),
        )
        .protected(
            Method::POST,
            "/community/posts",
            COMMUNITY_POST_CREATE_PERMISSION,
            post(create_post),
        )
        .protected(
            Method::PATCH,
            "/community/posts/{post_id}",
            COMMUNITY_POST_CREATE_PERMISSION,
            patch(update_post),
        )
        .protected(
            Method::DELETE,
            "/community/posts/{post_id}",
            COMMUNITY_POST_CREATE_PERMISSION,
            delete(delete_post),
        )
        .protected(
            Method::PUT,
            "/community/posts/{post_id}/reactions/interested",
            COMMUNITY_READ_PERMISSION,
            put(add_interested),
        )
        .protected(
            Method::DELETE,
            "/community/posts/{post_id}/reactions/interested",
            COMMUNITY_READ_PERMISSION,
            delete(remove_interested),
        )
        .with_state(state)
}
