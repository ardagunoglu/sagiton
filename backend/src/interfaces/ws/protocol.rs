use crate::domain::message::{
    MessageDeleted, MessageRecord, ThreadMessageDeleted, ThreadMessageRecord,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "t", content = "d", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ServerEvent {
    Ready {
        user_id: Uuid,
    },
    HeartbeatAck {
        ts: i64,
    },
    ChannelSubscribeAck {
        channel_id: Uuid,
    },
    ThreadSubscribeAck {
        thread_id: Uuid,
    },
    PresenceUpdate {
        user_id: Uuid,
        status: PresenceStatus,
    },
    MessageCreate {
        channel_id: Uuid,
        message: MessagePayload,
    },
    MessageUpdate {
        channel_id: Uuid,
        message: MessagePayload,
    },
    MessageDelete {
        channel_id: Uuid,
        message_id: Uuid,
        deleted_at: String,
    },
    ThreadMessageCreate {
        thread_id: Uuid,
        message: ThreadMessagePayload,
    },
    ThreadMessageUpdate {
        thread_id: Uuid,
        message: ThreadMessagePayload,
    },
    ThreadMessageDelete {
        thread_id: Uuid,
        message_id: Uuid,
        deleted_at: String,
    },
    ThreadRequestReceived {
        thread_id: Uuid,
        requester_user_id: Uuid,
        receiver_user_id: Uuid,
    },
    ThreadAccepted {
        thread_id: Uuid,
        accepted_by_user_id: Uuid,
        requester_user_id: Uuid,
        receiver_user_id: Uuid,
    },
    Typing {
        channel_id: Uuid,
        user_id: Uuid,
    },
    ThreadTyping {
        thread_id: Uuid,
        user_id: Uuid,
    },
    Error {
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PresenceStatus {
    Online,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagePayload {
    pub id: Uuid,
    pub channel_id: Uuid,
    pub author_id: Uuid,
    pub content: String,
    pub created_at: String,
    pub edited_at: Option<String>,
}

impl From<MessageRecord> for MessagePayload {
    fn from(value: MessageRecord) -> Self {
        Self {
            id: value.id,
            channel_id: value.channel_id,
            author_id: value.author_id,
            content: value.content,
            created_at: value
                .created_at
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
            edited_at: value.edited_at.and_then(|ts| {
                ts.format(&time::format_description::well_known::Rfc3339)
                    .ok()
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadMessagePayload {
    pub id: Uuid,
    pub thread_id: Uuid,
    pub author_id: Uuid,
    pub content: String,
    pub created_at: String,
    pub edited_at: Option<String>,
}

impl From<ThreadMessageRecord> for ThreadMessagePayload {
    fn from(value: ThreadMessageRecord) -> Self {
        Self {
            id: value.id,
            thread_id: value.thread_id,
            author_id: value.author_id,
            content: value.content,
            created_at: value
                .created_at
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
            edited_at: value.edited_at.and_then(|ts| {
                ts.format(&time::format_description::well_known::Rfc3339)
                    .ok()
            }),
        }
    }
}

impl From<MessageDeleted> for ServerEvent {
    fn from(value: MessageDeleted) -> Self {
        Self::MessageDelete {
            channel_id: value.channel_id,
            message_id: value.message_id,
            deleted_at: value
                .deleted_at
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        }
    }
}

impl From<ThreadMessageDeleted> for ServerEvent {
    fn from(value: ThreadMessageDeleted) -> Self {
        Self::ThreadMessageDelete {
            thread_id: value.thread_id,
            message_id: value.message_id,
            deleted_at: value
                .deleted_at
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", content = "d", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ClientCommand {
    Identify { token: String },
    Heartbeat { ts: i64 },
    SubscribeChannel { channel_id: Uuid },
    UnsubscribeChannel { channel_id: Uuid },
    TypingStart { channel_id: Uuid },
    SubscribeThread { thread_id: Uuid },
    UnsubscribeThread { thread_id: Uuid },
    ThreadTypingStart { thread_id: Uuid },
}
