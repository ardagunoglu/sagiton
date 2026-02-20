use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct CreateGuildRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateChannelRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateGuildInviteRequest {
    pub expires_in_seconds: Option<u64>,
    pub max_uses: Option<i32>,
}
