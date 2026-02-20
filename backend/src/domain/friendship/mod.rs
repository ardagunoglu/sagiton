use serde::Serialize;
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct FriendRequestRecord {
    pub from_user_id: Uuid,
    pub to_user_id: Uuid,
    pub status: String,
    pub created_at: OffsetDateTime,
    pub responded_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct FriendRequestSummary {
    pub from_user_id: Uuid,
    pub to_user_id: Uuid,
    pub from_username: String,
    pub to_username: String,
    pub status: String,
    pub created_at: OffsetDateTime,
    pub responded_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct FriendSummary {
    pub user_id: Uuid,
    pub username: String,
    pub created_at: OffsetDateTime,
}
