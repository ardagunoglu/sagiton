use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct CreateFriendRequest {
    pub to_user_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct ListFriendRequestsQuery {
    pub inbox: Option<String>,
}
