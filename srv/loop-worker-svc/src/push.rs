use anyhow::{Context, bail};
use diesel_async::AsyncConnection;
use google_cloud_auth::credentials::{AccessTokenCredentials, Builder};
use loop_svc_model::{
    conversation::{Conversation, ConversationMessage},
    push,
};
use std::time::Duration;

pub struct Fcm {
    client: reqwest::Client,
    credentials: AccessTokenCredentials,
    endpoint: String,
}

impl Fcm {
    pub fn from_env() -> anyhow::Result<Option<Self>> {
        let Some(project) = std::env::var("LOOP_FCM_PROJECT_ID")
            .ok()
            .filter(|s| !s.trim().is_empty())
        else {
            tracing::info!(
                event = "push.disabled",
                "FCM disabled: set LOOP_FCM_PROJECT_ID and Google application credentials to enable delivery"
            );
            return Ok(None);
        };
        if !project
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            bail!("invalid LOOP_FCM_PROJECT_ID");
        }
        let credentials = Builder::default()
            .with_scopes(["https://www.googleapis.com/auth/firebase.messaging"])
            .build_access_token_credentials()
            .context("failed to load FCM credentials")?;
        Ok(Some(Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .build()?,
            credentials,
            endpoint: format!("https://fcm.googleapis.com/v1/projects/{project}/messages:send"),
        }))
    }

    async fn send(
        &self,
        device: &push::PushDevice,
        conversation: &Conversation,
        message: &ConversationMessage,
    ) -> anyhow::Result<Outcome> {
        let token = self
            .credentials
            .access_token()
            .await
            .context("FCM credential refresh failed")?;
        let preview = if message.body.is_empty() {
            "[图片]".into()
        } else {
            message.body.chars().take(80).collect::<String>()
        };
        let body = serde_json::json!({"message":{
            "token":device.token,
            "notification":{"title":conversation.title,"body":preview},
            "data":{"conversation_id":conversation.conversation_id.to_string(),"message_id":message.message_id.to_string(),"seq":message.seq.to_string(),"user_id":device.user_id.to_string()},
            "android":{"priority":"high","ttl":"86400s","notification":{"channel_id":"messages","tag":message.message_id.to_string()}},
            "apns":{"headers":{"apns-push-type":"alert","apns-priority":"10","apns-expiration":(chrono::Utc::now().timestamp()+86400).to_string(),"apns-collapse-id":message.message_id.to_string()},"payload":{"aps":{"sound":"default"}}}
        }});
        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(token.token)
            .json(&body)
            .send()
            .await?;
        let status = response.status();
        if status.is_success() {
            return Ok(Outcome::Sent);
        }
        let retry_after = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| {
                s.parse::<i64>().ok().or_else(|| {
                    chrono::DateTime::parse_from_rfc2822(s)
                        .ok()
                        .map(|date| (date.timestamp() - chrono::Utc::now().timestamp()).max(0))
                })
            })
            .unwrap_or(60)
            .max(60);
        let body: serde_json::Value = response.json().await.unwrap_or_default();
        let unregistered = body
            .pointer("/error/details")
            .and_then(|v| v.as_array())
            .is_some_and(|details| {
                details
                    .iter()
                    .any(|d| d.get("errorCode").and_then(|v| v.as_str()) == Some("UNREGISTERED"))
            });
        // Do not log provider response bodies: they can echo device tokens or notification content.
        tracing::warn!(event="push.fcm.rejected", http_status=status.as_u16(), registration_id=%device.registration_id, message_id=%message.message_id, unregistered, "FCM rejected notification");
        if unregistered {
            Ok(Outcome::Unregistered)
        } else if status.as_u16() == 429 || status.is_server_error() {
            Ok(Outcome::Retry(retry_after))
        } else {
            Ok(Outcome::Failed)
        }
    }
}

enum Outcome {
    Sent,
    Unregistered,
    Retry(i64),
    Failed,
}

pub async fn run(fcm: Option<Fcm>) -> anyhow::Result<()> {
    let Some(fcm) = fcm else {
        return std::future::pending().await;
    };
    tracing::info!(event = "push.sender.started", "FCM delivery worker started");
    loop {
        let mut conn = loop_infra::db::pg_pool()?.get().await?;
        let processed = conn.transaction::<bool,anyhow::Error,_>(async |conn| {
            let Some(delivery) = push::next_delivery(conn).await? else { return Ok(false); };
            let Some(device) = push::device(delivery.registration_id, delivery.user_id, conn).await? else {
                push::finish(&delivery,"skipped",0,conn).await?; return Ok(true);
            };
            let Some(message) = ConversationMessage::select(delivery.message_id,conn).await? else {
                push::finish(&delivery,"skipped",0,conn).await?; return Ok(true);
            };
            let conversation = Conversation::select(message.conversation_id,conn).await?;
            let eligible = if let Some(ref conversation) = conversation {
                message.sender_id != delivery.user_id && message.deleted_at.is_none()
                    && message.created_at > chrono::Utc::now()-chrono::Duration::hours(24)
                    && conversation.can_read(delivery.user_id,conn).await?
                    && push::eligible(message.conversation_id,delivery.user_id,message.seq,conn).await?
            } else { false };
            if !eligible {
                push::finish(&delivery,"skipped",0,conn).await?; return Ok(true);
            }
            let started = std::time::Instant::now();
            let outcome = match tokio::time::timeout(Duration::from_secs(30), fcm.send(&device, &conversation.context("conversation missing")?, &message)).await {
                Ok(Ok(outcome)) => outcome,
                _ => {
                    tracing::warn!(event="push.transport.failed", message_id=%delivery.message_id, registration_id=%delivery.registration_id, "FCM transport or credential request failed; check connectivity and Google credentials");
                    Outcome::Retry(60)
                }
            };
            let (status,delay) = match outcome {
                Outcome::Sent => ("sent",0),
                Outcome::Unregistered => { push::disable(delivery.registration_id,conn).await?; ("skipped",0) },
                Outcome::Failed => ("failed",0),
                Outcome::Retry(delay) if delivery.attempts < 4 => ("pending", delay.max(60 * 2_i64.pow(delivery.attempts as u32))),
                Outcome::Retry(_) => ("failed",0),
            };
            push::finish(&delivery,status,delay,conn).await?;
            if status == "failed" {
                tracing::error!(event="push.delivery.failed", message_id=%delivery.message_id, registration_id=%delivery.registration_id, attempts=delivery.attempts+1, "notification delivery exhausted or permanently rejected");
            }
            tracing::info!(event="push.delivery.completed", message_id=%delivery.message_id, registration_id=%delivery.registration_id, status, elapsed_ms=started.elapsed().as_millis() as u64, attempts=delivery.attempts+1, "notification delivery attempt completed");
            Ok(true)
        }).await?;
        if !processed {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}
