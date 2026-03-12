use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateFriendRequest {
    pub to_user_id: Uuid,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ListFriendRequestsQuery {
    pub inbox: Option<String>,
}
