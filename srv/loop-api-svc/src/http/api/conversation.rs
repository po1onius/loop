use super::validation::{normalize_asset_ids, normalize_limit, parse_uuid, validate_image_assets};
use crate::{
    db_conn,
    http::{AppRoutes, AppState, AuthInfo, HttpErr, OptionExt, ResultExt, err_key::*},
};
use axum::{
    Json,
    extract::{Path, Query},
    routing::{get, post, put},
};
use chrono::Utc;
use diesel::{OptionalExtension, prelude::*};
use diesel_async::{AsyncConnection, RunQueryDsl};
use http::{Method, StatusCode};
use loop_dto::{
    ConversationCapabilitiesResp, ConversationKind, ConversationMessageResp, ConversationResp,
    ListConversationMessagesResp, ListConversationsResp, MarkConversationReadRequest,
    SendConversationMessageRequest, SetConversationSubscriptionRequest,
};
use loop_svc_model::{
    DieselConn, community::CommunityPost, conversation::*, event::Event,
    messaging::conversation::ConversationEvent, outbox::AsyncOutbox,
};
use serde::Deserialize;
use uuid::Uuid;
const COMMUNITY_READ_PERMISSION: &str = "conversation.read";
const COMMUNITY_MESSAGE_CREATE_PERMISSION: &str = "conversation.message.create";
const DEFAULT_MESSAGE_LIMIT: i64 = 50;
const MAX_MESSAGE_LIMIT: i64 = 100;
const MAX_MESSAGE_CHARS: usize = 2000;
const MAX_MESSAGE_IMAGES: usize = 4;
const MAX_MESSAGE_PREVIEW_CHARS: usize = 80;
#[derive(Debug, Deserialize)]
pub struct ListMessagesQuery {
    before_seq: Option<i64>,
    after_seq: Option<i64>,
    limit: Option<i64>,
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
    ensure_can_read(&record.conversation, auth.user_id, &mut conn).await?;
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
    ensure_can_read(&conversation, auth.user_id, &mut conn).await?;
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
    let conversation = Conversation::select(conversation_id, &mut conn)
        .await
        .internal(DB_ERROR)?
        .client(StatusCode::NOT_FOUND, CONVERSATION_NOT_FOUND)?;
    ensure_can_read(&conversation, auth.user_id, &mut conn).await?;
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
    let committed_id = conn
        .transaction::<Uuid, HttpErr, _>(async |conn| {
            let current = Conversation::lock_for_send(conversation_id, conn)
                .await
                .internal(DB_ERROR)?;
            ensure_can_read(&current, auth.user_id, conn).await?;
            if let Some(existing) =
                ConversationMessage::select_by_client_id(auth.user_id, client_message_id, conn)
                    .await
                    .internal(DB_ERROR)?
            {
                if existing.conversation_id != conversation_id {
                    return Err(HttpErr::client(StatusCode::CONFLICT, INVALID_INPUT));
                }
                return Ok(existing.message_id);
            }
            let conversation =
                Conversation::advance_for_message(conversation_id, &preview, now, conn)
                    .await
                    .internal(DB_ERROR)?
                    .client(StatusCode::CONFLICT, CONVERSATION_LOCKED)?;
            ensure_can_read(&conversation, auth.user_id, conn).await?;
            if conversation.status != "active" {
                return Err(HttpErr::client(StatusCode::CONFLICT, CONVERSATION_LOCKED));
            }
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
            AsyncOutbox::insert_conversation_event(
                ConversationEvent::message(conversation_id, message_id),
                conn,
            )
            .await
            .internal(DB_ERROR)?;
            Ok(message_id)
        })
        .await?;
    let record = ConversationMessage::select_record(committed_id, &mut conn)
        .await
        .internal(DB_ERROR)?
        .client(StatusCode::INTERNAL_SERVER_ERROR, DB_ERROR)?;
    let resp = to_message_resp(record);
    tracing::Span::current().record(
        "conversation.message.id",
        tracing::field::display(committed_id),
    );
    tracing::info!(event = "conversation.message.created", user_id = auth.user_id, %conversation_id, message_id=%committed_id, seq = resp.seq, image_count = image_asset_ids.len(), "conversation message committed");
    Ok((
        if committed_id == message_id {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(resp),
    ))
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
    ensure_can_read(&conversation, auth.user_id, &mut conn).await?;
    let last_read_seq = req.last_read_seq.min(conversation.last_seq);
    let previous = Conversation::select_with_state(conversation_id, auth.user_id, &mut conn)
        .await
        .internal(DB_ERROR)?;
    if previous
        .and_then(|r| r.read_state)
        .is_none_or(|r| r.last_read_seq < last_read_seq)
    {
        conn.transaction::<(), HttpErr, _>(async |conn| {
            ConversationReadState::mark_read(conversation_id, auth.user_id, last_read_seq, conn)
                .await
                .internal(DB_ERROR)?;
            AsyncOutbox::insert_conversation_event(
                ConversationEvent::changed(conversation_id, auth.user_id),
                conn,
            )
            .await
            .internal(DB_ERROR)?;
            Ok(())
        })
        .await?;
    }
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
    let mut conn = db_conn!();
    let conversation = Conversation::select(conversation_id, &mut conn)
        .await
        .internal(DB_ERROR)?
        .client(StatusCode::NOT_FOUND, CONVERSATION_NOT_FOUND)?;
    ensure_can_read(&conversation, auth.user_id, &mut conn).await?;
    if conversation.kind == "event_group" && !req.subscribed {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    conn.transaction::<(), HttpErr, _>(async |conn| {
        ConversationReadState::set_subscription(
            conversation_id,
            auth.user_id,
            req.subscribed,
            req.muted,
            conn,
        )
        .await
        .internal(DB_ERROR)?;
        AsyncOutbox::insert_conversation_event(
            ConversationEvent::changed(conversation_id, auth.user_id),
            conn,
        )
        .await
        .internal(DB_ERROR)?;
        Ok(())
    })
    .await?;
    tracing::info!(event = "conversation.subscription.updated", user_id = auth.user_id, %conversation_id, subscribed = req.subscribed, muted = req.muted, "conversation subscription updated");
    Ok(StatusCode::NO_CONTENT)
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
    // Every caller has checked access or selected authorized memberships.
    let can_read = record.conversation.status != "hidden";
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
        muted: state.as_ref().is_some_and(|s| s.muted),
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
    if conversation.kind == "event_group" {
        let id = conversation
            .subject_id
            .parse::<i64>()
            .map_err(|_| HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT))?;
        return Ok(Event::select_by_id(id, conn)
            .await
            .internal(DB_ERROR)?
            .map(|e| e.creator_id));
    }
    let post_id = parse_uuid(&conversation.subject_id)?;
    Ok(CommunityPost::select_record(post_id, conn)
        .await
        .internal(DB_ERROR)?
        .map(|record| record.post.author_id))
}

async fn ensure_can_read(
    conversation: &Conversation,
    user_id: i64,
    conn: &mut DieselConn,
) -> Result<(), HttpErr> {
    if !conversation
        .can_read(user_id, conn)
        .await
        .internal(DB_ERROR)?
    {
        tracing::warn!(event="conversation.access.denied", user_id, conversation_id=%conversation.conversation_id, "conversation access denied");
        return Err(HttpErr::client(StatusCode::FORBIDDEN, PERMISSION_DENIED));
    }
    Ok(())
}

pub async fn event_conversation(
    auth: AuthInfo,
    Path(event_id): Path<i64>,
) -> Result<Json<ConversationResp>, HttpErr> {
    let conversation = Conversation::for_event(event_id, &mut db_conn!())
        .await
        .optional()
        .internal(DB_ERROR)?
        .client(StatusCode::NOT_FOUND, CONVERSATION_NOT_FOUND)?;
    get_conversation(auth, Path(conversation.conversation_id.to_string())).await
}

pub async fn list_members(
    auth: AuthInfo,
    Path(id): Path<String>,
) -> Result<Json<Vec<loop_dto::ConversationMemberResp>>, HttpErr> {
    let mut conn = db_conn!();
    let conversation = Conversation::select(parse_uuid(&id)?, &mut conn)
        .await
        .internal(DB_ERROR)?
        .client(StatusCode::NOT_FOUND, CONVERSATION_NOT_FOUND)?;
    ensure_can_read(&conversation, auth.user_id, &mut conn).await?;
    if conversation.kind != "event_group" {
        return Err(HttpErr::client(StatusCode::BAD_REQUEST, INVALID_INPUT));
    }
    let owner = conversation_owner_id(&conversation, &mut conn).await?;
    let ids = conversation
        .recipients(&mut conn)
        .await
        .internal(DB_ERROR)?;
    use loop_svc_model::account::{User, users};
    let users = users::table
        .filter(users::user_id.eq_any(ids))
        .order(users::user_id.asc())
        .select(User::as_select())
        .load::<User>(&mut conn)
        .await
        .internal(DB_ERROR)?;
    let members = users
        .into_iter()
        .map(|user| loop_dto::ConversationMemberResp {
            is_owner: owner == Some(user.user_id),
            user_id: user.user_id.to_string(),
            username: user.username,
            avatar_asset_id: user.avatar_asset_id,
        })
        .collect();
    tracing::info!(event="conversation.members.loaded", conversation_id=%id, user_id=auth.user_id, "group members loaded");
    Ok(Json(members))
}

fn message_preview(body: &str, has_images: bool) -> String {
    let text = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() && has_images {
        return "[图片]".to_string();
    }
    text.chars().take(MAX_MESSAGE_PREVIEW_CHARS).collect()
}

pub fn route(state: AppState) -> AppRoutes {
    AppRoutes::<AppState>::new()
        .protected(
            Method::GET,
            "/events/{event_id}/conversation",
            COMMUNITY_READ_PERMISSION,
            get(event_conversation),
        )
        .protected(
            Method::GET,
            "/conversations/{conversation_id}/members",
            COMMUNITY_READ_PERMISSION,
            get(list_members),
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
