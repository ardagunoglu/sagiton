use crate::application::use_cases::guild_channel::{
    CreateChannelCommand, CreateGuildCommand, CreateGuildInviteCommand, JoinGuildCommand,
    JoinGuildInviteCommand, LeaveGuildCommand,
};
use crate::bootstrap::app_state::AppState;
use crate::interfaces::http::authentication::authenticated_user_id;
use crate::interfaces::http::dto::auth::MessageResponse;
use crate::interfaces::http::dto::guild_channel::{
    CreateChannelRequest, CreateGuildInviteRequest, CreateGuildRequest,
};
use crate::shared::error::AppResult;
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use uuid::Uuid;

/// Creates a guild for authenticated user.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `payload`: Guild creation payload.
pub async fn create_guild(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateGuildRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;

    let guild = state
        .guild_channel_use_case()
        .create_guild(CreateGuildCommand {
            user_id,
            name: payload.name,
        })
        .await?;

    Ok(Json(serde_json::json!({ "guild": guild })))
}

/// Lists guilds where authenticated user is member.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
pub async fn list_guilds(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;
    let guilds = state.guild_channel_use_case().list_guilds(user_id).await?;

    Ok(Json(serde_json::json!({ "guilds": guilds })))
}

/// Returns guild detail when user has membership.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `guild_id`: Target guild id from URL path.
pub async fn get_guild(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;
    let guild = state
        .guild_channel_use_case()
        .get_guild(guild_id, user_id)
        .await?;

    Ok(Json(serde_json::json!({ "guild": guild })))
}

/// Joins authenticated user to a guild.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `guild_id`: Target guild id from URL path.
pub async fn join_guild(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<Uuid>,
) -> AppResult<Json<MessageResponse>> {
    let user_id = authenticated_user_id(&state, &headers)?;

    state
        .guild_channel_use_case()
        .join_guild(JoinGuildCommand { user_id, guild_id })
        .await?;

    Ok(Json(MessageResponse {
        message: "joined guild",
    }))
}

/// Removes authenticated user from a guild.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `guild_id`: Target guild id from URL path.
pub async fn leave_guild(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<Uuid>,
) -> AppResult<Json<MessageResponse>> {
    let user_id = authenticated_user_id(&state, &headers)?;

    state
        .guild_channel_use_case()
        .leave_guild(LeaveGuildCommand { user_id, guild_id })
        .await?;

    Ok(Json(MessageResponse {
        message: "left guild",
    }))
}

/// Creates a text channel inside guild.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `guild_id`: Target guild id from URL path.
/// - `payload`: Channel creation payload.
pub async fn create_channel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<Uuid>,
    Json(payload): Json<CreateChannelRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;

    let channel = state
        .guild_channel_use_case()
        .create_channel(CreateChannelCommand {
            user_id,
            guild_id,
            name: payload.name,
        })
        .await?;

    Ok(Json(serde_json::json!({ "channel": channel })))
}

/// Lists channels from guild visible to authenticated user.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `guild_id`: Target guild id from URL path.
pub async fn list_channels(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;
    let channels = state
        .guild_channel_use_case()
        .list_channels(guild_id, user_id)
        .await?;

    Ok(Json(serde_json::json!({ "channels": channels })))
}

/// Returns channel details visible to authenticated user.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `channel_id`: Target channel id from URL path.
pub async fn get_channel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(channel_id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;
    let channel = state
        .guild_channel_use_case()
        .get_channel(channel_id, user_id)
        .await?;

    Ok(Json(serde_json::json!({ "channel": channel })))
}

/// Creates invite token for a guild member.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `guild_id`: Target guild id from URL path.
/// - `payload`: Invite create payload with optional expiry and usage cap.
pub async fn create_guild_invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(guild_id): Path<Uuid>,
    Json(payload): Json<CreateGuildInviteRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let actor_user_id = authenticated_user_id(&state, &headers)?;
    let invite = state
        .guild_channel_use_case()
        .create_guild_invite(CreateGuildInviteCommand {
            actor_user_id,
            guild_id,
            expires_in_seconds: payload.expires_in_seconds,
            max_uses: payload.max_uses,
        })
        .await?;

    Ok(Json(serde_json::json!({ "invite": invite })))
}

/// Joins a guild via invite token.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `token`: Invite token from URL path.
pub async fn join_guild_invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;
    let result = state
        .guild_channel_use_case()
        .join_guild_invite(JoinGuildInviteCommand { user_id, token })
        .await?;

    Ok(Json(
        serde_json::json!({ "guild": result.guild, "joined": result.joined }),
    ))
}
