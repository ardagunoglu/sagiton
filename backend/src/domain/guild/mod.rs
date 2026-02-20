use serde::Serialize;
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct GuildSummary {
    pub id: Uuid,
    pub name: String,
    pub owner_id: Uuid,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct GuildInviteRecord {
    pub token: String,
    pub guild_id: Uuid,
    pub created_by: Uuid,
    pub expires_at: OffsetDateTime,
    pub max_uses: Option<i32>,
    pub uses_count: i32,
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuildInviteJoinResult {
    pub guild: GuildSummary,
    pub joined: bool,
}
