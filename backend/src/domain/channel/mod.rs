use serde::Serialize;
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ChannelRecord {
    pub id: Uuid,
    pub guild_id: Uuid,
    pub name: String,
    #[sqlx(rename = "type")]
    pub channel_type: String,
    pub position: i32,
    pub created_at: OffsetDateTime,
    #[sqlx(default)]
    pub has_unread: bool,
}
