use serde::Serialize;
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct MessageRecord {
    pub id: Uuid,
    pub channel_id: Uuid,
    pub author_id: Uuid,
    pub content: String,
    pub created_at: OffsetDateTime,
    pub edited_at: Option<OffsetDateTime>,
    pub deleted_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageDeleted {
    pub channel_id: Uuid,
    pub message_id: Uuid,
    pub deleted_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ThreadMessageRecord {
    pub id: Uuid,
    pub thread_id: Uuid,
    pub author_id: Uuid,
    pub content: String,
    pub created_at: OffsetDateTime,
    pub edited_at: Option<OffsetDateTime>,
    pub deleted_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThreadMessageDeleted {
    pub thread_id: Uuid,
    pub message_id: Uuid,
    pub deleted_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ChannelReadState {
    pub user_id: Uuid,
    pub channel_id: Uuid,
    pub last_read_message_id: Option<Uuid>,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ThreadReadState {
    pub user_id: Uuid,
    pub thread_id: Uuid,
    pub last_read_message_id: Option<Uuid>,
    pub updated_at: OffsetDateTime,
}
