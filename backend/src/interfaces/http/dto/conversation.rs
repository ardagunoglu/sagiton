use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct CreateDirectThreadRequest {
    pub peer_user_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct ListDirectThreadsQuery {
    pub inbox: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateGroupThreadRequest {
    pub name: String,
    pub member_user_ids: Vec<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct AddGroupMemberRequest {
    pub user_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct CreateGroupInviteRequest {
    pub expires_in_seconds: Option<u64>,
    pub max_uses: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct CreateThreadMessageRequest {
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateThreadMessageRequest {
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct ListThreadMessagesQuery {
    pub before: Option<Uuid>,
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct MarkThreadReadRequest {
    pub last_read_message_id: Option<Uuid>,
}
