use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDirectThreadRequest {
    pub peer_user_id: Uuid,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ListDirectThreadsQuery {
    pub inbox: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateGroupThreadRequest {
    pub name: String,
    pub member_user_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AddGroupMemberRequest {
    pub user_id: Uuid,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateGroupInviteRequest {
    pub expires_in_seconds: Option<u64>,
    pub max_uses: Option<i32>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateThreadMessageRequest {
    pub content: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateThreadMessageRequest {
    pub content: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ListThreadMessagesQuery {
    pub before: Option<Uuid>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MarkThreadReadRequest {
    pub last_read_message_id: Option<Uuid>,
}
