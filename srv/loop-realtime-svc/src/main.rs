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
use loop_svc_model::conversation::Conversation;
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
    let claims = match authenticate_first_frame(authenticated, &state).await {
        Ok(claims) => claims,
        Err(error) => {
            tracing::warn!(event="realtime.authentication.failed", reason=%error, "websocket authentication failed");
            let _ = socket.close().await;
            return;
        }
    };
    if let Err(error) = stream_session(&mut socket, &state, &claims).await {
        tracing::warn!(event="realtime.session.failed", user_id=claims.user_id, error_source=?error, "realtime session ended");
    }
    let _ = socket.close().await;
    tracing::info!(
        event = "realtime.connection.closed",
        user_id = claims.user_id,
        "user websocket closed"
    );
}

async fn stream_session(
    socket: &mut WebSocket,
    state: &AppState,
    claims: &Claims,
) -> anyhow::Result<()> {
    let mut pubsub = state.redis_client.get_async_pubsub().await?;
    pubsub
        .subscribe(format!("loop:user:{}", claims.user_id))
        .await?;
    let (mut subscriptions, mut messages) = pubsub.split();
    let mut watched = std::collections::HashSet::new();
    send_json(socket, &serde_json::json!({"type":"ready"})).await?;
    tracing::info!(
        event = "realtime.connection.opened",
        user_id = claims.user_id,
        "user websocket authenticated"
    );
    let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let remaining =
        (claims.exp as u64).saturating_sub(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs());
    let expiration = tokio::time::sleep(Duration::from_secs(remaining));
    tokio::pin!(expiration);
    loop {
        tokio::select! {
            frame = socket.recv() => {
                match frame {
                    Some(Ok(Message::Text(text))) => {
                        #[derive(Deserialize)]
                        struct Subscription { #[serde(rename="type")] action: String, conversation_id: Uuid }
                        let command = match serde_json::from_str::<Subscription>(&text) {
                            Ok(command) => command,
                            Err(_) => { send_json(socket, &serde_json::json!({"type":"error","reason":"invalid_frame"})).await?; continue; }
                        };
                        let id = command.conversation_id;
                        if command.action == "unsubscribe" {
                            watched.remove(&id);
                            subscriptions.unsubscribe(format!("loop:conversation:{id}")).await?;
                        } else if command.action == "subscribe" {
                            let mut conn = loop_infra::db::pg_pool()?.get().await?;
                            let allowed = match Conversation::select(id, &mut conn).await? {
                                Some(conversation) => conversation.can_read(claims.user_id, &mut conn).await?,
                                None => false,
                            };
                            if !allowed || (watched.len() >= 32 && !watched.contains(&id)) {
                                tracing::warn!(event="realtime.subscription.denied", user_id=claims.user_id, conversation_id=%id, "conversation subscription denied");
                                send_json(socket, &serde_json::json!({"type":"error","conversation_id":id,"reason":"forbidden"})).await?;
                                continue;
                            }
                            watched.insert(id);
                            subscriptions.subscribe(format!("loop:conversation:{id}")).await?;
                            send_json(socket, &serde_json::json!({"type":"subscribed","conversation_id":id})).await?;
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => socket.send(Message::Pong(payload)).await?,
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(error)) => return Err(error.into()),
                    _ => {}
                }
            }
            message = messages.next() => {
                let Some(message) = message else { bail!("Redis subscription stream ended"); };
                let payload: String = message.get_payload()?;
                let event: serde_json::Value = serde_json::from_str(&payload)?;
                // Recheck current access, including for a previously authorized connection.
                if let Some(id) = event.get("conversation_id").and_then(|v|v.as_str()).and_then(|v|Uuid::parse_str(v).ok()) {
                    let mut conn = loop_infra::db::pg_pool()?.get().await?;
                    let allowed = match Conversation::select(id, &mut conn).await? {
                        Some(c) => c.can_read(claims.user_id, &mut conn).await?, None => false,
                    };
                    if allowed { socket.send(Message::Text(payload.into())).await?; }
                }
            }
            _ = heartbeat.tick() => socket.send(Message::Ping(Vec::new().into())).await?,
            _ = &mut expiration => break,
        }
    }
    Ok(())
}

async fn authenticate_first_frame(
    received: Result<Option<Result<Message, axum::Error>>, tokio::time::error::Elapsed>,
    state: &AppState,
) -> anyhow::Result<Claims> {
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
    let mut conn = loop_infra::db::pg_pool()?.get().await?;
    let user = loop_svc_model::account::User::select_by_user_id(claims.user_id, &mut conn)
        .await?
        .context("user not found")?;
    if user.role != claims.role {
        bail!("user role changed");
    }
    Ok(claims)
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
