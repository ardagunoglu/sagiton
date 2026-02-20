use crate::application::use_cases::message::{
    CreateMessageCommand, DeleteMessageCommand, EditMessageCommand,
    ListMessagesQuery as UseCaseListQuery, MarkChannelReadCommand,
};
use crate::bootstrap::app_state::AppState;
use crate::interfaces::http::authentication::authenticated_user_id;
use crate::interfaces::http::dto::auth::MessageResponse;
use crate::interfaces::http::dto::message::{
    CreateMessageRequest, ListMessagesQuery, MarkChannelReadRequest, UpdateMessageRequest,
};
use crate::interfaces::ws::protocol::{MessagePayload, ServerEvent};
use crate::shared::error::{AppError, AppResult};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use uuid::Uuid;

/// Creates channel message and publishes MESSAGE_CREATE event.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `channel_id`: Target channel id from URL path.
/// - `payload`: Message create payload.
pub async fn create_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(channel_id): Path<Uuid>,
    Json(payload): Json<CreateMessageRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;

    let rl_key = format!("rl:message:{user_id}:{channel_id}");
    let allowed = state.allow_rate_limit(&rl_key, 30, 10).await;
    if !allowed {
        return Err(AppError::rate_limited("message rate limit exceeded"));
    }

    let message = state
        .message_use_case()
        .create_message(CreateMessageCommand {
            user_id,
            channel_id,
            content: payload.content,
        })
        .await?;

    state
        .publish_channel_event(
            channel_id,
            ServerEvent::MessageCreate {
                channel_id,
                message: MessagePayload::from(message.clone()),
            },
        )
        .await;

    Ok(Json(serde_json::json!({ "message": message })))
}

/// Lists channel messages with stable cursor pagination.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `channel_id`: Target channel id from URL path.
/// - `query`: Query payload with `before` cursor and `limit`.
pub async fn list_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(channel_id): Path<Uuid>,
    Query(query): Query<ListMessagesQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;

    let messages = state
        .message_use_case()
        .list_messages(UseCaseListQuery {
            user_id,
            channel_id,
            before: query.before,
            limit: query.limit,
        })
        .await?;

    Ok(Json(serde_json::json!({ "messages": messages })))
}

/// Edits message content and publishes MESSAGE_UPDATE event.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `message_id`: Target message id from URL path.
/// - `payload`: Message update payload.
pub async fn update_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(message_id): Path<Uuid>,
    Json(payload): Json<UpdateMessageRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;

    let message = state
        .message_use_case()
        .edit_message(EditMessageCommand {
            user_id,
            message_id,
            content: payload.content,
        })
        .await?;

    state
        .publish_channel_event(
            message.channel_id,
            ServerEvent::MessageUpdate {
                channel_id: message.channel_id,
                message: MessagePayload::from(message.clone()),
            },
        )
        .await;

    Ok(Json(serde_json::json!({ "message": message })))
}

/// Soft deletes message and publishes MESSAGE_DELETE event.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `message_id`: Target message id from URL path.
pub async fn delete_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(message_id): Path<Uuid>,
) -> AppResult<Json<MessageResponse>> {
    let user_id = authenticated_user_id(&state, &headers)?;

    let deleted = state
        .message_use_case()
        .delete_message(DeleteMessageCommand {
            user_id,
            message_id,
        })
        .await?;

    state
        .publish_channel_event(deleted.channel_id, ServerEvent::from(deleted))
        .await;

    Ok(Json(MessageResponse {
        message: "message deleted",
    }))
}

/// Updates read marker for a channel member.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `channel_id`: Target channel id from URL path.
/// - `payload`: Optional read marker payload.
pub async fn mark_channel_read(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(channel_id): Path<Uuid>,
    Json(payload): Json<MarkChannelReadRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;
    let read = state
        .message_use_case()
        .mark_channel_read(MarkChannelReadCommand {
            user_id,
            channel_id,
            last_read_message_id: payload.last_read_message_id,
        })
        .await?;

    Ok(Json(serde_json::json!({ "read": read })))
}
