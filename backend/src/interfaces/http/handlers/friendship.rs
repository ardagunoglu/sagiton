use crate::application::use_cases::friendship::{
    CreateFriendRequestCommand, FriendRequestInboxFilter, RespondFriendRequestCommand,
};
use crate::bootstrap::app_state::AppState;
use crate::interfaces::http::authentication::authenticated_user_id;
use crate::interfaces::http::dto::auth::MessageResponse;
use crate::interfaces::http::dto::friendship::{CreateFriendRequest, ListFriendRequestsQuery};
use crate::shared::error::{AppError, AppResult};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use uuid::Uuid;

/// Sends friend request from authenticated user to target user.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `payload`: Friend request create payload.
pub async fn create_friend_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateFriendRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let from_user_id = authenticated_user_id(&state, &headers)?;
    let request = state
        .friendship_use_case()
        .create_friend_request(CreateFriendRequestCommand {
            from_user_id,
            to_user_id: payload.to_user_id,
        })
        .await?;

    Ok(Json(serde_json::json!({ "request": request })))
}

/// Accepts incoming friend request.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `from_user_id`: Request sender id from URL path.
pub async fn accept_friend_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(from_user_id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;
    let friend = state
        .friendship_use_case()
        .accept_friend_request(RespondFriendRequestCommand {
            user_id,
            from_user_id,
        })
        .await?;

    Ok(Json(serde_json::json!({ "friend": friend })))
}

/// Rejects incoming friend request.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `from_user_id`: Request sender id from URL path.
pub async fn reject_friend_request(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(from_user_id): Path<Uuid>,
) -> AppResult<Json<MessageResponse>> {
    let user_id = authenticated_user_id(&state, &headers)?;
    state
        .friendship_use_case()
        .reject_friend_request(RespondFriendRequestCommand {
            user_id,
            from_user_id,
        })
        .await?;

    Ok(Json(MessageResponse {
        message: "friend request rejected",
    }))
}

/// Lists accepted friends of authenticated user.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
pub async fn list_friends(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;
    let friends = state.friendship_use_case().list_friends(user_id).await?;

    Ok(Json(serde_json::json!({ "friends": friends })))
}

/// Lists pending friend requests for authenticated user's inbox/outbox.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `query`: Optional inbox selector (`pending` or `outbox`).
pub async fn list_friend_requests(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListFriendRequestsQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;
    let filter = parse_friend_request_filter(query.inbox.as_deref())?;
    let requests = state
        .friendship_use_case()
        .list_friend_requests(user_id, filter)
        .await?;

    Ok(Json(serde_json::json!({ "requests": requests })))
}

fn parse_friend_request_filter(raw: Option<&str>) -> AppResult<FriendRequestInboxFilter> {
    let Some(value) = raw else {
        return Ok(FriendRequestInboxFilter::Pending);
    };

    match value.trim().to_ascii_lowercase().as_str() {
        "pending" => Ok(FriendRequestInboxFilter::Pending),
        "outbox" => Ok(FriendRequestInboxFilter::Outbox),
        _ => Err(AppError::validation(
            "inbox must be one of: pending, outbox",
        )),
    }
}
