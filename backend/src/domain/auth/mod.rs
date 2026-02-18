use serde::Serialize;
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
#[allow(dead_code)]
pub struct UserRecord {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub password_hash: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct UserProfile {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Debug, Clone)]
pub struct SessionContext {
    pub user_agent: Option<String>,
    pub ip: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SessionCreate {
    pub user_id: Uuid,
    pub refresh_token_hash: String,
    pub expires_at: OffsetDateTime,
    pub context: SessionContext,
}

#[derive(Debug, Clone)]
pub struct SessionRotate {
    pub old_refresh_token_hash: String,
    pub new_refresh_token_hash: String,
    pub expires_at: OffsetDateTime,
    pub context: SessionContext,
}
