use std::{
    fs,
    process::ExitCode,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, bail};
use axum::{
    Router,
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::Response,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use loop_infra::{
    config::InfraConfig,
    observability::{ObservabilityConfig, init as init_observability, metrics_handler},
};
use loop_svc_model::community::Conversation;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const SERVICE_NAME: &str = "loop-realtime-svc";
const AUTHENTICATION_TIMEOUT: Duration = Duration::from_secs(10);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(25);

#[derive(Clone)]
struct AppState {
    jwt_decoding_key: Arc<DecodingKey>,
    redis_client: redis::Client,
}

#[derive(Debug, Deserialize)]
struct Claims {
    #[allow(dead_code)]
    exp: usize,
    user_id: i64,
    #[allow(dead_code)]
    perm_ver: u32,
    #[allow(dead_code)]
    role: String,
}

#[derive(Debug, Deserialize)]
struct AuthenticateFrame {
    #[serde(rename = "type")]
    frame_type: String,
    access_token: String,
    conversation_id: String,
}

#[derive(Debug, Serialize)]
struct ServerFrame<'a> {
    #[serde(rename = "type")]
    frame_type: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    conversation_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
}

#[tokio::main]
async fn main() -> ExitCode {
    let _observability = match init_observability(ObservabilityConfig::new(
        SERVICE_NAME,
        env!("CARGO_PKG_VERSION"),
    )) {
        Ok(guard) => guard,
        Err(error) => {
            eprintln!("failed to initialize observability: {error:?}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(error) = run().await {
        tracing::error!(
            event = "service.exit",
            error_source = ?error,
            error_chain = %format_error_chain(&error),
            "realtime service exited with error"
        );
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

async fn run() -> anyhow::Result<()> {
    let infra = InfraConfig::from_env()?;
    let redis_url = infra.redis_conn.clone();
    infra.init()?;
    let jwt_public_key = read_secret("LOOP_JWT_RSA_PUB_KEY", "LOOP_JWT_RSA_PUB_KEY_FILE")?;
    let state = AppState {
        jwt_decoding_key: Arc::new(
            DecodingKey::from_rsa_pem(jwt_public_key.as_bytes())
                .context("failed to parse JWT public key")?,
        ),
        // Pub/Sub 连接是长生命周期专用连接，不应占用普通命令连接池。
        redis_client: redis::Client::open(redis_url)
            .context("failed to build Redis Pub/Sub client")?,
    };
    let app = Router::new()
        .route("/metrics", get(metrics_handler))
        .route("/loop/realtime", get(upgrade_websocket))
        .with_state(state);
    let http_addr = std::env::var("LOOP_HTTP_ADDR")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "127.0.0.1:3010".to_string());
    let listener = tokio::net::TcpListener::bind(&http_addr)
        .await
        .context("failed to bind realtime HTTP listener")?;
    tracing::info!(event = "service.start", service_name = SERVICE_NAME, %http_addr, "realtime service started");
    axum::serve(listener, app)
        .await
        .context("realtime HTTP server stopped with error")?;
    Ok(())
}

async fn upgrade_websocket(State(state): State<AppState>, upgrade: WebSocketUpgrade) -> Response {
    upgrade
        .max_message_size(16 * 1024)
        .on_upgrade(move |socket| serve_socket(socket, state))
}

async fn serve_socket(mut socket: WebSocket, state: AppState) {
    let authenticated = tokio::time::timeout(AUTHENTICATION_TIMEOUT, socket.recv()).await;
    let (claims, conversation_id) = match authenticate_first_frame(authenticated, &state).await {
        Ok(result) => result,
        Err(error) => {
            tracing::warn!(event = "realtime.authentication.failed", reason = %error, "websocket authentication failed");
            let _ = send_json(
                &mut socket,
                &ServerFrame {
                    frame_type: "error",
                    conversation_id: None,
                    reason: Some("unauthorized"),
                },
            )
            .await;
            let _ = socket.close().await;
            return;
        }
    };

    let conversation_id_text = conversation_id.to_string();
    let channel = format!("loop:conversation:{conversation_id}");
    let mut pubsub = match state.redis_client.get_async_pubsub().await {
        Ok(pubsub) => pubsub,
        Err(error) => {
            tracing::error!(event = "realtime.redis.connect_failed", user_id = claims.user_id, error_source = ?error, "failed to open Redis Pub/Sub connection");
            let _ = socket.close().await;
            return;
        }
    };
    if let Err(error) = pubsub.subscribe(&channel).await {
        tracing::error!(event = "realtime.redis.subscribe_failed", user_id = claims.user_id, %channel, error_source = ?error, "failed to subscribe realtime channel");
        let _ = socket.close().await;
        return;
    }
    if send_json(
        &mut socket,
        &ServerFrame {
            frame_type: "subscribed",
            conversation_id: Some(&conversation_id_text),
            reason: None,
        },
    )
    .await
    .is_err()
    {
        return;
    }
    tracing::info!(event = "realtime.connection.opened", user_id = claims.user_id, %conversation_id, "conversation websocket subscribed");

    let mut redis_messages = pubsub.on_message();
    let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // WebSocket 建立后也不能无限延长 access token 的权限。到达 JWT exp 时主动
    // 断开，客户端会先刷新 token 再重连，从而同步最新的权限版本和账户状态。
    let now_epoch_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let remaining_token_ttl = (claims.exp as u64).saturating_sub(now_epoch_secs);
    let token_expiration = tokio::time::sleep(Duration::from_secs(remaining_token_ttl));
    tokio::pin!(token_expiration);
    loop {
        tokio::select! {
            socket_message = socket.recv() => {
                match socket_message {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() { break; }
                    }
                    Some(Ok(Message::Text(text))) => {
                        tracing::debug!(event = "realtime.client.frame_ignored", user_id = claims.user_id, frame_bytes = text.len(), "ignored frame after subscription");
                    }
                    Some(Ok(_)) => {}
                    Some(Err(error)) => {
                        tracing::debug!(event = "realtime.socket.read_failed", user_id = claims.user_id, error_source = ?error, "websocket read failed");
                        break;
                    }
                }
            }
            redis_message = redis_messages.next() => {
                let Some(redis_message) = redis_message else { break; };
                match redis_message.get_payload::<String>() {
                    Ok(payload) => {
                        if socket.send(Message::Text(payload.into())).await.is_err() { break; }
                    }
                    Err(error) => tracing::warn!(event = "realtime.redis.payload_invalid", user_id = claims.user_id, %conversation_id, error_source = ?error, "invalid Redis realtime payload"),
                }
            }
            _ = heartbeat.tick() => {
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() { break; }
            }
            _ = &mut token_expiration => {
                tracing::info!(event = "realtime.access_token.expired", user_id = claims.user_id, %conversation_id, "closing websocket at access token expiration");
                break;
            }
        }
    }
    tracing::info!(event = "realtime.connection.closed", user_id = claims.user_id, %conversation_id, "conversation websocket closed");
}

async fn authenticate_first_frame(
    received: Result<Option<Result<Message, axum::Error>>, tokio::time::error::Elapsed>,
    state: &AppState,
) -> anyhow::Result<(Claims, Uuid)> {
    let message = received
        .context("authentication frame timed out")?
        .context("websocket closed before authentication")?
        .context("failed to read authentication frame")?;
    let text = match message {
        Message::Text(text) => text,
        _ => bail!("first websocket frame must be text"),
    };
    let frame: AuthenticateFrame =
        serde_json::from_str(&text).context("invalid authentication frame")?;
    if frame.frame_type != "authenticate" {
        bail!("first websocket frame must authenticate");
    }
    let mut validation = Validation::new(Algorithm::RS256);
    validation.validate_aud = false;
    validation.leeway = 0;
    let claims = decode::<Claims>(&frame.access_token, &state.jwt_decoding_key, &validation)
        .context("access token rejected")?
        .claims;
    let conversation_id =
        Uuid::parse_str(&frame.conversation_id).context("invalid conversation id")?;

    // 当前帖子讨论是 open 会话。先在服务端确认会话存在且可读，避免客户端直接
    // 订阅任意 Redis channel。将来开放 restricted 活动群聊时在这里补成员校验。
    let pool = loop_infra::db::pg_pool()?;
    let mut conn = pool
        .get()
        .await
        .context("failed to get database connection")?;
    let conversation = Conversation::select(conversation_id, &mut conn)
        .await
        .context("failed to load conversation")?
        .context("conversation not found")?;
    if conversation.status == "hidden" || conversation.access_mode != "open" {
        bail!("conversation is not readable by this realtime connection");
    }
    Ok((claims, conversation_id))
}

async fn send_json<T: Serialize>(socket: &mut WebSocket, frame: &T) -> Result<(), axum::Error> {
    socket
        .send(Message::Text(
            serde_json::to_string(frame)
                .expect("server frame serialization must succeed")
                .into(),
        ))
        .await
}

fn read_secret(value_key: &str, file_key: &str) -> anyhow::Result<String> {
    let value = std::env::var(value_key)
        .ok()
        .filter(|value| !value.trim().is_empty());
    let file = std::env::var(file_key)
        .ok()
        .filter(|value| !value.trim().is_empty());
    match (value, file) {
        (Some(_), Some(_)) => bail!("set either {value_key} or {file_key}, not both"),
        (Some(value), None) => Ok(value),
        (None, Some(path)) => fs::read_to_string(&path)
            .with_context(|| format!("failed to read secret file: {path}"))
            .and_then(|value| {
                if value.trim().is_empty() {
                    bail!("secret file is empty: {path}")
                }
                Ok(value)
            }),
        (None, None) => bail!("{value_key} or {file_key} is required"),
    }
}

fn format_error_chain(error: &anyhow::Error) -> String {
    error
        .chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ")
}
