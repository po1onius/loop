use crate::{
    db_conn,
    http::{AppRoutes, AppState, AuthInfo, HttpErr, OptionExt, ResultExt, err_key::*},
    redis_conn,
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
    ConversationCapabilitiesResp, ConversationKind, ConversationMessageResp, ConversationResp,
    CreateCommunityPostRequest, ListCommunityPostsResp, ListConversationMessagesResp,
    ListConversationsResp, MarkConversationReadRequest, SendConversationMessageRequest,
    SetConversationSubscriptionRequest, UpdateCommunityPostRequest,
};
use loop_svc_model::{
    DieselConn,
    community::{
        CommunityPost, CommunityPostChanges, CommunityPostRecord, CommunitySection, Conversation,
        ConversationMessage, ConversationMessageRecord, ConversationReadState, ConversationRecord,
        NewCommunityPost, NewCommunityPostEventLink, NewConversation, NewConversationMessage,
        add_interested_reaction, delete_post_event_links, insert_post_event_link,
        remove_interested_reaction, select_interested_post_ids,
    },
    event::{Event, MediaAsset},
};
use redis::AsyncCommands;
use serde::Deserialize;
use std::collections::HashSet;
use uuid::Uuid;

const COMMUNITY_READ_PERMISSION: &str = "community.read";
const COMMUNITY_POST_CREATE_PERMISSION: &str = "community.post.create";
const COMMUNITY_MESSAGE_CREATE_PERMISSION: &str = "community.message.create";
const DEFAULT_POST_LIMIT: i64 = 20;
const MAX_POST_LIMIT: i64 = 50;
const DEFAULT_MESSAGE_LIMIT: i64 = 50;
const MAX_MESSAGE_LIMIT: i64 = 100;
const MAX_POST_TITLE_CHARS: usize = 120;
const MAX_POST_BODY_CHARS: usize = 10_000;
const MAX_POST_IMAGES: usize = 9;
const MAX_MESSAGE_CHARS: usize = 2_000;
const MAX_MESSAGE_IMAGES: usize = 4;
const MAX_MESSAGE_PREVIEW_CHARS: usize = 80;

#[derive(Debug, Deserialize)]
pub struct ListPostsQuery {
    section_id: Option<String>,
    sort: Option<String>,
    cursor: Option<String>,
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct ListMessagesQuery {
    before_seq: Option<i64>,
    after_seq: Option<i64>,
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
        CommunityPost::soft_delete_owned(post_id, auth.user_id, conn)
            .await
            .internal(DB_ERROR)?;
        Conversation::lock(record.post.discussion_conversation_id, conn)
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

#[tracing::instrument(name = "conversation.list", skip_all, fields(user.id = auth.user_id))]
pub async fn list_conversations(auth: AuthInfo) -> Result<Json<ListConversationsResp>, HttpErr> {
    let records = Conversation::list_subscribed(auth.user_id, &mut db_conn!())
        .await
        .internal(DB_ERROR)?;
    Ok(Json(ListConversationsResp {
        items: records
            .into_iter()
            .map(|record| to_conversation_resp(record, auth.user_id, false))
            .collect::<Result<Vec<_>, _>>()?,
    }))
}

#[tracing::instrument(name = "conversation.get", skip_all, fields(user.id = auth.user_id, conversation.id = %conversation_id))]
pub async fn get_conversation(
    auth: AuthInfo,
    Path(conversation_id): Path<String>,
) -> Result<Json<ConversationResp>, HttpErr> {
    let conversation_id = parse_uuid(&conversation_id)?;
    let mut conn = db_conn!();
    let record = Conversation::select_with_state(conversation_id, auth.user_id, &mut conn)
        .await
        .internal(DB_ERROR)?
        .client(StatusCode::NOT_FOUND, CONVERSATION_NOT_FOUND)?;
    ensure_can_read(&record.conversation)?;
    let can_manage =
        conversation_owner_id(&record.conversation, &mut conn).await? == Some(auth.user_id);
    Ok(Json(to_conversation_resp(
        record,
        auth.user_id,
        can_manage,
    )?))
}

#[tracing::instrument(name = "conversation.message.list", skip_all, fields(user.id = auth.user_id, conversation.id = %conversation_id))]
pub async fn list_messages(
    auth: AuthInfo,
    Path(conversation_id): Path<String>,
    Query(query): Query<ListMessagesQuery>,
) -> Result<Json<ListConversationMessagesResp>, HttpErr> {
    let conversation_id = parse_uuid(&conversation_id)?;
    if query.before_seq.is_some_and(|seq| seq <= 0)
        || query.after_seq.is_some_and(|seq| seq < 0)
        || (query.before_seq.is_some() && query.after_seq.is_some())
    {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    let limit = normalize_limit(query.limit, DEFAULT_MESSAGE_LIMIT, MAX_MESSAGE_LIMIT)?;
    let mut conn = db_conn!();
    let conversation = Conversation::select(conversation_id, &mut conn)
        .await
        .internal(DB_ERROR)?
        .client(StatusCode::NOT_FOUND, CONVERSATION_NOT_FOUND)?;
    ensure_can_read(&conversation)?;
    let loading_forward = query.after_seq.is_some();
    let mut records = if let Some(after_seq) = query.after_seq {
        ConversationMessage::list_records_after(conversation_id, after_seq, limit + 1, &mut conn)
            .await
            .internal(DB_ERROR)?
    } else {
        ConversationMessage::list_records(conversation_id, query.before_seq, limit + 1, &mut conn)
            .await
            .internal(DB_ERROR)?
    };
    let has_more = records.len() as i64 > limit;
    if has_more {
        records.pop();
    }
    let page_edge_seq = has_more
        .then(|| records.last().map(|record| record.message.seq))
        .flatten();
    if !loading_forward {
        records.reverse();
    }
    tracing::info!(event = "conversation.messages.loaded", user_id = auth.user_id, %conversation_id, item_count = records.len(), has_more, loading_forward, "conversation messages loaded");
    Ok(Json(ListConversationMessagesResp {
        items: records.into_iter().map(to_message_resp).collect(),
        next_before_seq: (!loading_forward).then_some(page_edge_seq).flatten(),
        next_after_seq: loading_forward.then_some(page_edge_seq).flatten(),
    }))
}

#[tracing::instrument(name = "conversation.message.send", skip_all, fields(user.id = auth.user_id, conversation.id = %conversation_id, conversation.message.id = tracing::field::Empty))]
pub async fn send_message(
    auth: AuthInfo,
    Path(conversation_id): Path<String>,
    Json(req): Json<SendConversationMessageRequest>,
) -> Result<(StatusCode, Json<ConversationMessageResp>), HttpErr> {
    let conversation_id = parse_uuid(&conversation_id)?;
    let client_message_id = parse_uuid(&req.client_message_id)?;
    let quote_message_id = req
        .quote_message_id
        .as_deref()
        .map(parse_uuid)
        .transpose()?;
    let body = req.body.trim().to_string();
    let image_asset_ids = normalize_asset_ids(req.image_asset_ids, MAX_MESSAGE_IMAGES)?;
    if body.chars().count() > MAX_MESSAGE_CHARS || (body.is_empty() && image_asset_ids.is_empty()) {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    validate_image_assets(auth.user_id, &image_asset_ids).await?;
    let mut conn = db_conn!();
    if let Some(existing) =
        ConversationMessage::select_by_client_id(auth.user_id, client_message_id, &mut conn)
            .await
            .internal(DB_ERROR)?
    {
        if existing.conversation_id != conversation_id {
            return Err(HttpErr::client(StatusCode::CONFLICT, INVALID_INPUT));
        }
        let record = ConversationMessage::select_record(existing.message_id, &mut conn)
            .await
            .internal(DB_ERROR)?
            .client(StatusCode::NOT_FOUND, CONVERSATION_NOT_FOUND)?;
        return Ok((StatusCode::OK, Json(to_message_resp(record))));
    }
    if let Some(quote_id) = quote_message_id {
        let quoted = ConversationMessage::select(quote_id, &mut conn)
            .await
            .internal(DB_ERROR)?
            .client(StatusCode::BAD_REQUEST, INVALID_INPUT)?;
        if quoted.conversation_id != conversation_id {
            return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
        }
    }
    let message_id = Uuid::now_v7();
    let now = Utc::now();
    let preview = message_preview(&body, !image_asset_ids.is_empty());
    conn.transaction::<(), HttpErr, _>(async |conn| {
        let conversation = Conversation::advance_for_message(conversation_id, &preview, now, conn)
            .await
            .internal(DB_ERROR)?
            .client(StatusCode::CONFLICT, CONVERSATION_LOCKED)?;
        ensure_can_send(&conversation)?;
        ConversationMessage::insert(
            NewConversationMessage {
                message_id,
                conversation_id,
                seq: conversation.last_seq,
                sender_id: auth.user_id,
                client_message_id,
                message_type: if body.is_empty() { "image" } else { "text" },
                body: &body,
                image_asset_ids: &image_asset_ids,
                quote_message_id,
            },
            conn,
        )
        .await
        .internal(DB_ERROR)?;
        if conversation.kind == "post_thread" {
            CommunityPost::update_discussion_stats(conversation_id, now, conn)
                .await
                .internal(DB_ERROR)?;
        }
        ConversationReadState::upsert(
            conversation_id,
            auth.user_id,
            conversation.last_seq,
            true,
            false,
            conn,
        )
        .await
        .internal(DB_ERROR)?;
        Ok(())
    })
    .await?;
    let record = ConversationMessage::select_record(message_id, &mut conn)
        .await
        .internal(DB_ERROR)?
        .client(StatusCode::INTERNAL_SERVER_ERROR, DB_ERROR)?;
    let resp = to_message_resp(record);
    tracing::Span::current().record(
        "conversation.message.id",
        tracing::field::display(message_id),
    );
    tracing::info!(event = "conversation.message.created", user_id = auth.user_id, %conversation_id, %message_id, seq = resp.seq, image_count = image_asset_ids.len(), "conversation message committed");
    publish_message_event(&resp).await;
    Ok((StatusCode::CREATED, Json(resp)))
}

#[tracing::instrument(name = "conversation.read.update", skip_all, fields(user.id = auth.user_id, conversation.id = %conversation_id))]
pub async fn mark_conversation_read(
    auth: AuthInfo,
    Path(conversation_id): Path<String>,
    Json(req): Json<MarkConversationReadRequest>,
) -> Result<StatusCode, HttpErr> {
    let conversation_id = parse_uuid(&conversation_id)?;
    if req.last_read_seq < 0 {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    let mut conn = db_conn!();
    let conversation = Conversation::select(conversation_id, &mut conn)
        .await
        .internal(DB_ERROR)?
        .client(StatusCode::NOT_FOUND, CONVERSATION_NOT_FOUND)?;
    ensure_can_read(&conversation)?;
    let last_read_seq = req.last_read_seq.min(conversation.last_seq);
    ConversationReadState::mark_read(conversation_id, auth.user_id, last_read_seq, &mut conn)
        .await
        .internal(DB_ERROR)?;
    tracing::info!(event = "conversation.read.updated", user_id = auth.user_id, %conversation_id, last_read_seq, "conversation read cursor updated");
    Ok(StatusCode::NO_CONTENT)
}

#[tracing::instrument(name = "conversation.subscription.update", skip_all, fields(user.id = auth.user_id, conversation.id = %conversation_id))]
pub async fn set_conversation_subscription(
    auth: AuthInfo,
    Path(conversation_id): Path<String>,
    Json(req): Json<SetConversationSubscriptionRequest>,
) -> Result<StatusCode, HttpErr> {
    let conversation_id = parse_uuid(&conversation_id)?;
    let conversation = Conversation::select(conversation_id, &mut db_conn!())
        .await
        .internal(DB_ERROR)?
        .client(StatusCode::NOT_FOUND, CONVERSATION_NOT_FOUND)?;
    ensure_can_read(&conversation)?;
    ConversationReadState::set_subscription(
        conversation_id,
        auth.user_id,
        req.subscribed,
        req.muted,
        &mut db_conn!(),
    )
    .await
    .internal(DB_ERROR)?;
    tracing::info!(event = "conversation.subscription.updated", user_id = auth.user_id, %conversation_id, subscribed = req.subscribed, muted = req.muted, "conversation subscription updated");
    Ok(StatusCode::NO_CONTENT)
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

async fn validate_image_assets(user_id: i64, asset_ids: &[String]) -> Result<(), HttpErr> {
    validate_image_assets_with_conn(user_id, asset_ids, &mut db_conn!()).await
}

async fn validate_image_assets_with_conn(
    user_id: i64,
    asset_ids: &[String],
    conn: &mut DieselConn,
) -> Result<(), HttpErr> {
    if asset_ids.is_empty() {
        return Ok(());
    }
    let assets = MediaAsset::select_uploaded_by_ids_for_owner(asset_ids, user_id, conn)
        .await
        .internal(DB_ERROR)?;
    if assets.len() != asset_ids.len()
        || assets
            .iter()
            .any(|asset| !asset.mime_type.starts_with("image/"))
    {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, MEDIA_NOT_FOUND));
    }
    Ok(())
}

fn normalize_asset_ids(asset_ids: Vec<String>, max_count: usize) -> Result<Vec<String>, HttpErr> {
    let mut seen = HashSet::new();
    let normalized = asset_ids
        .into_iter()
        .map(|asset_id| asset_id.trim().to_string())
        .filter(|asset_id| !asset_id.is_empty())
        .filter(|asset_id| seen.insert(asset_id.clone()))
        .collect::<Vec<_>>();
    if normalized.len() > max_count {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    Ok(normalized)
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

fn to_conversation_resp(
    record: ConversationRecord,
    _viewer_id: i64,
    can_manage: bool,
) -> Result<ConversationResp, HttpErr> {
    let kind = match record.conversation.kind.as_str() {
        "post_thread" => ConversationKind::PostThread,
        "event_group" => ConversationKind::EventGroup,
        _ => return Err(HttpErr::client(StatusCode::INTERNAL_SERVER_ERROR, DB_ERROR)),
    };
    let state = record.read_state;
    let last_read_seq = state.as_ref().map_or(0, |state| state.last_read_seq);
    let subscribed = state.as_ref().is_some_and(|state| state.subscribed);
    let can_read =
        record.conversation.status != "hidden" && record.conversation.access_mode == "open";
    let can_send = can_read && record.conversation.status == "active";
    Ok(ConversationResp {
        conversation_id: record.conversation.conversation_id.to_string(),
        kind,
        subject_id: record.conversation.subject_id,
        title: record.conversation.title,
        status: record.conversation.status.clone(),
        last_seq: record.conversation.last_seq,
        message_count: record.conversation.message_count,
        last_message_preview: record.conversation.last_message_preview,
        last_message_at: record
            .conversation
            .last_message_at
            .map(|value| value.to_rfc3339()),
        last_read_seq,
        unread_count: (record.conversation.last_seq - last_read_seq).max(0),
        subscribed,
        capabilities: ConversationCapabilitiesResp {
            can_read,
            can_send,
            can_upload: can_send,
            can_quote: can_send,
            can_manage,
            read_only_reason: (!can_send).then(|| {
                if record.conversation.status == "locked" {
                    "讨论已关闭".to_string()
                } else {
                    "当前没有参与权限".to_string()
                }
            }),
        },
    })
}

fn to_message_resp(record: ConversationMessageRecord) -> ConversationMessageResp {
    ConversationMessageResp {
        message_id: record.message.message_id.to_string(),
        conversation_id: record.message.conversation_id.to_string(),
        seq: record.message.seq,
        sender_id: record.message.sender_id.to_string(),
        sender_username: record.sender_username,
        sender_avatar_asset_id: record.sender_avatar_asset_id,
        client_message_id: record.message.client_message_id.to_string(),
        message_type: record.message.message_type,
        body: if record.message.deleted_at.is_some() {
            "消息已删除".to_string()
        } else {
            record.message.body
        },
        image_asset_ids: if record.message.deleted_at.is_some() {
            Vec::new()
        } else {
            record.message.image_asset_ids
        },
        quote_message_id: record
            .message
            .quote_message_id
            .map(|value| value.to_string()),
        created_at: record.message.created_at.to_rfc3339(),
        edited_at: record.message.edited_at.map(|value| value.to_rfc3339()),
        deleted_at: record.message.deleted_at.map(|value| value.to_rfc3339()),
    }
}

async fn conversation_owner_id(
    conversation: &Conversation,
    conn: &mut DieselConn,
) -> Result<Option<i64>, HttpErr> {
    if conversation.kind != "post_thread" {
        return Ok(None);
    }
    let post_id = parse_uuid(&conversation.subject_id)?;
    Ok(CommunityPost::select_record(post_id, conn)
        .await
        .internal(DB_ERROR)?
        .map(|record| record.post.author_id))
}

fn ensure_can_read(conversation: &Conversation) -> Result<(), HttpErr> {
    if conversation.status == "hidden" {
        return Err(HttpErr::client(
            StatusCode::NOT_FOUND,
            CONVERSATION_NOT_FOUND,
        ));
    }
    if conversation.access_mode != "open" {
        return Err(HttpErr::client(StatusCode::FORBIDDEN, PERMISSION_DENIED));
    }
    Ok(())
}

fn ensure_can_send(conversation: &Conversation) -> Result<(), HttpErr> {
    ensure_can_read(conversation)?;
    if conversation.status != "active" {
        return Err(HttpErr::client(StatusCode::CONFLICT, CONVERSATION_LOCKED));
    }
    Ok(())
}

async fn publish_message_event(message: &ConversationMessageResp) {
    let payload = serde_json::json!({
        "type": "conversation.message_created",
        "conversation_id": message.conversation_id,
        "seq": message.seq,
        "message_id": message.message_id,
    })
    .to_string();
    let channel = format!("loop:conversation:{}", message.conversation_id);
    let result = async {
        let mut redis = redis_conn!();
        redis
            .publish::<_, _, i64>(&channel, payload)
            .await
            .internal(REDIS_ERROR)
    }
    .await;
    match result {
        Ok(receiver_count) => {
            tracing::info!(event = "conversation.message.published", %channel, receiver_count, "conversation message realtime event published")
        }
        Err(error) => {
            tracing::warn!(event = "conversation.message.publish_failed", %channel, error_source = ?error, "conversation message was committed but realtime publication failed")
        }
    }
}

fn message_preview(body: &str, has_images: bool) -> String {
    let text = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() && has_images {
        return "[图片]".to_string();
    }
    text.chars().take(MAX_MESSAGE_PREVIEW_CHARS).collect()
}

fn normalize_limit(value: Option<i64>, default: i64, max: i64) -> Result<i64, HttpErr> {
    let value = value.unwrap_or(default);
    if value <= 0 || value > max {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    Ok(value)
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

fn parse_uuid(value: &str) -> Result<Uuid, HttpErr> {
    Uuid::parse_str(value.trim())
        .map_err(|_| HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT))
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
        .protected(
            Method::GET,
            "/conversations",
            COMMUNITY_READ_PERMISSION,
            get(list_conversations),
        )
        .protected(
            Method::GET,
            "/conversations/{conversation_id}",
            COMMUNITY_READ_PERMISSION,
            get(get_conversation),
        )
        .protected(
            Method::GET,
            "/conversations/{conversation_id}/messages",
            COMMUNITY_READ_PERMISSION,
            get(list_messages),
        )
        .protected(
            Method::POST,
            "/conversations/{conversation_id}/messages",
            COMMUNITY_MESSAGE_CREATE_PERMISSION,
            post(send_message),
        )
        .protected(
            Method::PUT,
            "/conversations/{conversation_id}/read",
            COMMUNITY_READ_PERMISSION,
            put(mark_conversation_read),
        )
        .protected(
            Method::PUT,
            "/conversations/{conversation_id}/subscription",
            COMMUNITY_READ_PERMISSION,
            put(set_conversation_subscription),
        )
        .with_state(state)
}
