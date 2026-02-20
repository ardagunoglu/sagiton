use crate::application::use_cases::conversation::{
    AcceptDirectThreadCommand, AddGroupMemberCommand, CreateDirectThreadCommand,
    CreateGroupInviteCommand, CreateGroupThreadCommand, CreateThreadMessageCommand,
    DeleteThreadMessageCommand, DirectInboxFilter, EditThreadMessageCommand,
    JoinGroupInviteCommand, LeaveGroupCommand, ListThreadMessagesQuery as UseCaseListQuery,
    MarkThreadReadCommand, RejectDirectThreadCommand,
};
use crate::bootstrap::app_state::AppState;
use crate::interfaces::http::authentication::authenticated_user_id;
use crate::interfaces::http::dto::auth::MessageResponse;
use crate::interfaces::http::dto::conversation::{
    AddGroupMemberRequest, CreateDirectThreadRequest, CreateGroupInviteRequest,
    CreateGroupThreadRequest, CreateThreadMessageRequest, ListDirectThreadsQuery,
    ListThreadMessagesQuery, MarkThreadReadRequest, UpdateThreadMessageRequest,
};
use crate::interfaces::ws::protocol::{ServerEvent, ThreadMessagePayload};
use crate::shared::error::{AppError, AppResult};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use uuid::Uuid;

/// Creates or returns direct thread between authenticated user and peer.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `payload`: Direct thread create payload.
pub async fn create_direct_thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateDirectThreadRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;

    let result = state
        .conversation_use_case()
        .create_direct_thread(CreateDirectThreadCommand {
            user_id,
            peer_user_id: payload.peer_user_id,
        })
        .await?;

    if result.created
        && result.thread.status == "PENDING"
        && result.thread.requested_by == Some(user_id)
    {
        state
            .publish_user_event(
                result.thread.peer_user_id,
                ServerEvent::ThreadRequestReceived {
                    thread_id: result.thread.id,
                    requester_user_id: user_id,
                    receiver_user_id: result.thread.peer_user_id,
                },
            )
            .await;
    }

    Ok(Json(serde_json::json!({
        "thread": result.thread,
        "created": result.created
    })))
}

/// Lists direct threads for authenticated user.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `query`: Optional inbox filter (`pending`).
pub async fn list_direct_threads(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListDirectThreadsQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;
    let filter = parse_inbox_filter(query.inbox.as_deref())?;

    let threads = state
        .conversation_use_case()
        .list_direct_threads(user_id, filter)
        .await?;

    Ok(Json(serde_json::json!({ "threads": threads })))
}

/// Returns direct thread detail if authenticated user is participant.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `thread_id`: Direct thread id from URL path.
pub async fn get_direct_thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(thread_id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;
    let thread = state
        .conversation_use_case()
        .get_direct_thread(thread_id, user_id)
        .await?;

    Ok(Json(serde_json::json!({ "thread": thread })))
}

/// Accepts pending direct thread request.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `thread_id`: Direct thread id from URL path.
pub async fn accept_direct_thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(thread_id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;
    let transition = state
        .conversation_use_case()
        .accept_direct_thread(AcceptDirectThreadCommand { user_id, thread_id })
        .await?;

    let event = ServerEvent::ThreadAccepted {
        thread_id,
        accepted_by_user_id: user_id,
        requester_user_id: transition.requester_user_id,
        receiver_user_id: transition.receiver_user_id,
    };
    state
        .publish_user_event(transition.requester_user_id, event.clone())
        .await;
    state
        .publish_user_event(transition.receiver_user_id, event)
        .await;

    let thread = state
        .conversation_use_case()
        .get_direct_thread(thread_id, user_id)
        .await?;

    Ok(Json(serde_json::json!({ "thread": thread })))
}

/// Rejects pending direct thread request.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `thread_id`: Direct thread id from URL path.
pub async fn reject_direct_thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(thread_id): Path<Uuid>,
) -> AppResult<Json<MessageResponse>> {
    let user_id = authenticated_user_id(&state, &headers)?;
    state
        .conversation_use_case()
        .reject_direct_thread(RejectDirectThreadCommand { user_id, thread_id })
        .await?;

    Ok(Json(MessageResponse {
        message: "direct thread rejected",
    }))
}

/// Creates group thread and initial members.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `payload`: Group thread create payload.
pub async fn create_group_thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateGroupThreadRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let owner_id = authenticated_user_id(&state, &headers)?;

    let thread = state
        .conversation_use_case()
        .create_group_thread(CreateGroupThreadCommand {
            owner_id,
            name: payload.name,
            member_user_ids: payload.member_user_ids,
        })
        .await?;

    Ok(Json(serde_json::json!({ "thread": thread })))
}

/// Lists group threads for authenticated user.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
pub async fn list_group_threads(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;
    let threads = state
        .conversation_use_case()
        .list_group_threads(user_id)
        .await?;

    Ok(Json(serde_json::json!({ "threads": threads })))
}

/// Returns group thread detail if authenticated user is member.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `thread_id`: Group thread id from URL path.
pub async fn get_group_thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(thread_id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;
    let thread = state
        .conversation_use_case()
        .get_group_thread(thread_id, user_id)
        .await?;

    Ok(Json(serde_json::json!({ "thread": thread })))
}

/// Adds member to group thread when caller is group owner.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `thread_id`: Group thread id from URL path.
/// - `payload`: Target member id payload.
pub async fn add_group_member(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(thread_id): Path<Uuid>,
    Json(payload): Json<AddGroupMemberRequest>,
) -> AppResult<Json<MessageResponse>> {
    let actor_user_id = authenticated_user_id(&state, &headers)?;

    state
        .conversation_use_case()
        .add_group_member(AddGroupMemberCommand {
            actor_user_id,
            thread_id,
            user_id: payload.user_id,
        })
        .await?;

    Ok(Json(MessageResponse {
        message: "member added",
    }))
}

/// Removes authenticated user from group thread.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `thread_id`: Group thread id from URL path.
pub async fn leave_group_thread(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(thread_id): Path<Uuid>,
) -> AppResult<Json<MessageResponse>> {
    let user_id = authenticated_user_id(&state, &headers)?;

    state
        .conversation_use_case()
        .leave_group(LeaveGroupCommand { user_id, thread_id })
        .await?;

    Ok(Json(MessageResponse {
        message: "left group",
    }))
}

/// Creates invite token for a group thread.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `thread_id`: Group thread id from URL path.
/// - `payload`: Invite create payload with optional expiry and usage cap.
pub async fn create_group_invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(thread_id): Path<Uuid>,
    Json(payload): Json<CreateGroupInviteRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let actor_user_id = authenticated_user_id(&state, &headers)?;
    let invite = state
        .conversation_use_case()
        .create_group_invite(CreateGroupInviteCommand {
            actor_user_id,
            thread_id,
            expires_in_seconds: payload.expires_in_seconds,
            max_uses: payload.max_uses,
        })
        .await?;

    Ok(Json(serde_json::json!({ "invite": invite })))
}

/// Joins group thread via invite token.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `token`: Invite token from URL path.
pub async fn join_group_invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(token): Path<String>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;
    let thread = state
        .conversation_use_case()
        .join_group_invite(JoinGroupInviteCommand { user_id, token })
        .await?;

    Ok(Json(serde_json::json!({ "thread": thread })))
}

/// Updates read marker for a thread member.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `thread_id`: Target thread id from URL path.
/// - `payload`: Optional read marker payload.
pub async fn mark_thread_read(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(thread_id): Path<Uuid>,
    Json(payload): Json<MarkThreadReadRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;
    let read = state
        .conversation_use_case()
        .mark_thread_read(MarkThreadReadCommand {
            user_id,
            thread_id,
            last_read_message_id: payload.last_read_message_id,
        })
        .await?;

    Ok(Json(serde_json::json!({ "read": read })))
}

/// Creates thread message and publishes THREAD_MESSAGE_CREATE event.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `thread_id`: Target thread id from URL path.
/// - `payload`: Thread message create payload.
pub async fn create_thread_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(thread_id): Path<Uuid>,
    Json(payload): Json<CreateThreadMessageRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;

    let rl_key = format!("rl:thread-message:{user_id}:{thread_id}");
    let allowed = state.allow_rate_limit(&rl_key, 30, 10).await;
    if !allowed {
        return Err(AppError::rate_limited("message rate limit exceeded"));
    }

    let message = state
        .conversation_use_case()
        .create_thread_message(CreateThreadMessageCommand {
            user_id,
            thread_id,
            content: payload.content,
        })
        .await?;

    state
        .publish_thread_event(
            thread_id,
            ServerEvent::ThreadMessageCreate {
                thread_id,
                message: ThreadMessagePayload::from(message.clone()),
            },
        )
        .await;

    Ok(Json(serde_json::json!({ "message": message })))
}

/// Lists thread messages with stable cursor pagination.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `thread_id`: Target thread id from URL path.
/// - `query`: Query payload with `before` cursor and `limit`.
pub async fn list_thread_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(thread_id): Path<Uuid>,
    Query(query): Query<ListThreadMessagesQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;

    let messages = state
        .conversation_use_case()
        .list_thread_messages(UseCaseListQuery {
            user_id,
            thread_id,
            before: query.before,
            limit: query.limit,
        })
        .await?;

    Ok(Json(serde_json::json!({ "messages": messages })))
}

/// Edits thread message and publishes THREAD_MESSAGE_UPDATE event.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `message_id`: Target message id from URL path.
/// - `payload`: Thread message update payload.
pub async fn update_thread_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(message_id): Path<Uuid>,
    Json(payload): Json<UpdateThreadMessageRequest>,
) -> AppResult<Json<serde_json::Value>> {
    let user_id = authenticated_user_id(&state, &headers)?;

    let message = state
        .conversation_use_case()
        .edit_thread_message(EditThreadMessageCommand {
            user_id,
            message_id,
            content: payload.content,
        })
        .await?;

    state
        .publish_thread_event(
            message.thread_id,
            ServerEvent::ThreadMessageUpdate {
                thread_id: message.thread_id,
                message: ThreadMessagePayload::from(message.clone()),
            },
        )
        .await;

    Ok(Json(serde_json::json!({ "message": message })))
}

/// Soft deletes thread message and publishes THREAD_MESSAGE_DELETE event.
///
/// # Parameters
/// - `headers`: Request headers containing bearer token.
/// - `message_id`: Target thread message id from URL path.
pub async fn delete_thread_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(message_id): Path<Uuid>,
) -> AppResult<Json<MessageResponse>> {
    let user_id = authenticated_user_id(&state, &headers)?;

    let deleted = state
        .conversation_use_case()
        .delete_thread_message(DeleteThreadMessageCommand {
            user_id,
            message_id,
        })
        .await?;

    state
        .publish_thread_event(deleted.thread_id, ServerEvent::from(deleted))
        .await;

    Ok(Json(MessageResponse {
        message: "message deleted",
    }))
}

fn parse_inbox_filter(inbox: Option<&str>) -> AppResult<DirectInboxFilter> {
    match inbox.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(DirectInboxFilter::All),
        Some(value) if value.eq_ignore_ascii_case("pending") => Ok(DirectInboxFilter::Pending),
        Some(_) => Err(AppError::validation("invalid inbox filter")),
    }
}
