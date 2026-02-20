use serde::Serialize;
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct DirectThreadSummary {
    pub id: Uuid,
    pub peer_user_id: Uuid,
    pub peer_username: String,
    pub status: String,
    pub requested_by: Option<Uuid>,
    pub accepted_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    #[sqlx(default)]
    pub has_unread: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DirectThreadUpsertResult {
    pub thread: DirectThreadSummary,
    pub created: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DirectThreadTransition {
    pub thread_id: Uuid,
    pub requester_user_id: Uuid,
    pub receiver_user_id: Uuid,
    pub accepted_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct GroupThreadSummary {
    pub id: Uuid,
    pub name: String,
    pub owner_id: Uuid,
    pub created_at: OffsetDateTime,
    #[sqlx(default)]
    pub has_unread: bool,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct GroupInviteRecord {
    pub token: String,
    pub thread_id: Uuid,
    pub created_by: Uuid,
    pub expires_at: OffsetDateTime,
    pub max_uses: Option<i32>,
    pub used_count: i32,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, FromRow)]
pub struct ThreadAccessRecord {
    pub thread_id: Uuid,
    pub kind: String,
    pub direct_status: String,
    pub requested_by: Option<Uuid>,
}
