use serde::Deserialize;
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateGuildRequest {
    pub name: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateChannelRequest {
    pub name: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateGuildInviteRequest {
    pub expires_in_seconds: Option<u64>,
    pub max_uses: Option<i32>,
}
