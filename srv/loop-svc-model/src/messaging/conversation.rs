use super::MessageContract;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConversationEvent {
    pub conversation_id: Uuid,
    pub message_id: Option<Uuid>,
    pub user_id: Option<i64>,
}
impl ConversationEvent {
    pub fn changed(conversation_id: Uuid, user_id: i64) -> Self {
        Self {
            conversation_id,
            user_id: Some(user_id),
            message_id: None,
        }
    }
    pub fn message(conversation_id: Uuid, message_id: Uuid) -> Self {
        Self {
            conversation_id,
            message_id: Some(message_id),
            user_id: None,
        }
    }
}
impl MessageContract for ConversationEvent {
    const MESSAGE_TYPE: &'static str = "com.loop.conversation.changed.v1";
    const ROUTING_KEY: &'static str = "conversation.changed.v1";
    fn subject(&self) -> String {
        format!("conversations/{}", self.conversation_id)
    }
    fn validate_data(&self) -> Result<(), &'static str> {
        if self.conversation_id.is_nil()
            || self.message_id.is_some() == self.user_id.is_some()
            || self.user_id.is_some_and(|id| id <= 0)
            || self.message_id.is_some_and(|id| id.is_nil())
        {
            return Err("invalid conversation event");
        }
        Ok(())
    }
}
