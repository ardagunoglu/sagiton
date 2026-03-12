#![allow(dead_code)]

use crate::interfaces::http::dto::auth::{
    LoginRequest, LogoutRequest, MessageResponse, RefreshRequest, RegisterRequest,
};
use crate::interfaces::http::dto::conversation::{
    AddGroupMemberRequest, CreateDirectThreadRequest, CreateGroupInviteRequest,
    CreateGroupThreadRequest, CreateThreadMessageRequest, MarkThreadReadRequest,
    UpdateThreadMessageRequest,
};
use crate::interfaces::http::dto::friendship::CreateFriendRequest;
use crate::interfaces::http::dto::guild_channel::{
    CreateChannelRequest, CreateGuildInviteRequest, CreateGuildRequest,
};
use crate::interfaces::http::dto::message::{
    CreateMessageRequest, MarkChannelReadRequest, UpdateMessageRequest,
};
use axum::Json;
use serde::Serialize;
use utoipa::{OpenApi, ToSchema};
use uuid::Uuid;

/// Serves OpenAPI specification as JSON.
pub async fn openapi_spec() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Sagiton Backend API",
        version = "0.1.0-beta.1",
        description = "HTTP API contract for Sagiton backend."
    ),
    paths(
        health_endpoint,
        openapi_endpoint,
        metrics_endpoint,
        ws_endpoint,
        auth_register_endpoint,
        auth_login_endpoint,
        auth_refresh_endpoint,
        auth_logout_endpoint,
        auth_me_endpoint,
        friends_list_endpoint,
        friends_create_request_endpoint,
        friends_list_requests_endpoint,
        friends_accept_request_endpoint,
        friends_reject_request_endpoint,
        guilds_list_endpoint,
        guilds_create_endpoint,
        guilds_get_endpoint,
        guilds_join_endpoint,
        guilds_leave_endpoint,
        guilds_create_invite_endpoint,
        guild_invites_join_endpoint,
        guild_channels_list_endpoint,
        guild_channels_create_endpoint,
        channels_get_endpoint,
        direct_threads_list_endpoint,
        direct_threads_create_endpoint,
        direct_threads_get_endpoint,
        direct_threads_accept_endpoint,
        direct_threads_reject_endpoint,
        group_threads_list_endpoint,
        group_threads_create_endpoint,
        group_threads_get_endpoint,
        group_threads_add_member_endpoint,
        group_threads_leave_endpoint,
        group_threads_create_invite_endpoint,
        group_invites_join_endpoint,
        thread_messages_list_endpoint,
        thread_messages_create_endpoint,
        thread_read_mark_endpoint,
        thread_message_update_endpoint,
        thread_message_delete_endpoint,
        channel_messages_list_endpoint,
        channel_messages_create_endpoint,
        channel_read_mark_endpoint,
        channel_message_update_endpoint,
        channel_message_delete_endpoint
    ),
    tags(
        (name = "System", description = "Service and observability endpoints"),
        (name = "Auth", description = "Authentication and session lifecycle"),
        (name = "Friendship", description = "Friend request and friends management"),
        (name = "Guilds", description = "Guild and guild channel operations"),
        (name = "Threads", description = "Direct and group thread operations"),
        (name = "Messages", description = "Channel and thread message operations")
    ),
    components(
        schemas(
            RegisterRequest,
            LoginRequest,
            RefreshRequest,
            LogoutRequest,
            MessageResponse,
            CreateGuildRequest,
            CreateChannelRequest,
            CreateGuildInviteRequest,
            CreateFriendRequest,
            CreateMessageRequest,
            UpdateMessageRequest,
            MarkChannelReadRequest,
            CreateDirectThreadRequest,
            CreateGroupThreadRequest,
            AddGroupMemberRequest,
            CreateGroupInviteRequest,
            CreateThreadMessageRequest,
            UpdateThreadMessageRequest,
            MarkThreadReadRequest,
            HealthResponseDoc,
            UserProfileDoc,
            TokenPairDoc,
            RegisterResponseDoc,
            MeResponseDoc,
            GuildSummaryDoc,
            GuildEnvelopeDoc,
            GuildListResponseDoc,
            ChannelRecordDoc,
            ChannelEnvelopeDoc,
            ChannelListResponseDoc,
            GuildInviteRecordDoc,
            GuildInviteResponseDoc,
            GuildInviteJoinResponseDoc,
            FriendSummaryDoc,
            FriendsListResponseDoc,
            FriendRequestRecordDoc,
            FriendRequestSummaryDoc,
            FriendRequestCreateResponseDoc,
            FriendRequestListResponseDoc,
            FriendAcceptResponseDoc,
            DirectThreadSummaryDoc,
            DirectThreadCreateResponseDoc,
            DirectThreadEnvelopeDoc,
            DirectThreadListResponseDoc,
            GroupThreadSummaryDoc,
            GroupThreadEnvelopeDoc,
            GroupThreadListResponseDoc,
            GroupInviteRecordDoc,
            GroupInviteResponseDoc,
            MessageRecordDoc,
            ThreadMessageRecordDoc,
            ChannelMessageEnvelopeDoc,
            ThreadMessageEnvelopeDoc,
            ChannelMessageListResponseDoc,
            ThreadMessageListResponseDoc,
            ChannelReadStateDoc,
            ThreadReadStateDoc,
            ChannelReadResponseDoc,
            ThreadReadResponseDoc,
            ApiErrorResponseDoc,
            ApiErrorPayloadDoc
        )
    )
)]
pub struct ApiDoc;

#[derive(Debug, Serialize, ToSchema)]
struct HealthResponseDoc {
    status: String,
    service: String,
    port: u16,
}

#[derive(Debug, Serialize, ToSchema)]
struct UserProfileDoc {
    id: Uuid,
    username: String,
    email: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
struct TokenPairDoc {
    access_token: String,
    refresh_token: String,
}

#[derive(Debug, Serialize, ToSchema)]
struct RegisterResponseDoc {
    user: UserProfileDoc,
}

#[derive(Debug, Serialize, ToSchema)]
struct MeResponseDoc {
    user: UserProfileDoc,
}

#[derive(Debug, Serialize, ToSchema)]
struct GuildSummaryDoc {
    id: Uuid,
    name: String,
    owner_id: Uuid,
    created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
struct GuildEnvelopeDoc {
    guild: GuildSummaryDoc,
}

#[derive(Debug, Serialize, ToSchema)]
struct GuildListResponseDoc {
    guilds: Vec<GuildSummaryDoc>,
}

#[derive(Debug, Serialize, ToSchema)]
struct ChannelRecordDoc {
    id: Uuid,
    guild_id: Uuid,
    name: String,
    channel_type: String,
    position: i32,
    created_at: String,
    has_unread: bool,
}

#[derive(Debug, Serialize, ToSchema)]
struct ChannelEnvelopeDoc {
    channel: ChannelRecordDoc,
}

#[derive(Debug, Serialize, ToSchema)]
struct ChannelListResponseDoc {
    channels: Vec<ChannelRecordDoc>,
}

#[derive(Debug, Serialize, ToSchema)]
struct GuildInviteRecordDoc {
    token: String,
    guild_id: Uuid,
    created_by: Uuid,
    expires_at: String,
    max_uses: Option<i32>,
    uses_count: i32,
    created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
struct GuildInviteResponseDoc {
    invite: GuildInviteRecordDoc,
}

#[derive(Debug, Serialize, ToSchema)]
struct GuildInviteJoinResponseDoc {
    guild: GuildSummaryDoc,
    joined: bool,
}

#[derive(Debug, Serialize, ToSchema)]
struct FriendSummaryDoc {
    user_id: Uuid,
    username: String,
    created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
struct FriendsListResponseDoc {
    friends: Vec<FriendSummaryDoc>,
}

#[derive(Debug, Serialize, ToSchema)]
struct FriendRequestRecordDoc {
    from_user_id: Uuid,
    to_user_id: Uuid,
    status: String,
    created_at: String,
    responded_at: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
struct FriendRequestSummaryDoc {
    from_user_id: Uuid,
    to_user_id: Uuid,
    from_username: String,
    to_username: String,
    status: String,
    created_at: String,
    responded_at: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
struct FriendRequestCreateResponseDoc {
    request: FriendRequestRecordDoc,
}

#[derive(Debug, Serialize, ToSchema)]
struct FriendRequestListResponseDoc {
    requests: Vec<FriendRequestSummaryDoc>,
}

#[derive(Debug, Serialize, ToSchema)]
struct FriendAcceptResponseDoc {
    friend: FriendSummaryDoc,
}

#[derive(Debug, Serialize, ToSchema)]
struct DirectThreadSummaryDoc {
    id: Uuid,
    peer_user_id: Uuid,
    peer_username: String,
    status: String,
    requested_by: Option<Uuid>,
    accepted_at: Option<String>,
    created_at: String,
    has_unread: bool,
}

#[derive(Debug, Serialize, ToSchema)]
struct DirectThreadCreateResponseDoc {
    thread: DirectThreadSummaryDoc,
    created: bool,
}

#[derive(Debug, Serialize, ToSchema)]
struct DirectThreadEnvelopeDoc {
    thread: DirectThreadSummaryDoc,
}

#[derive(Debug, Serialize, ToSchema)]
struct DirectThreadListResponseDoc {
    threads: Vec<DirectThreadSummaryDoc>,
}

#[derive(Debug, Serialize, ToSchema)]
struct GroupThreadSummaryDoc {
    id: Uuid,
    name: String,
    owner_id: Uuid,
    created_at: String,
    has_unread: bool,
}

#[derive(Debug, Serialize, ToSchema)]
struct GroupThreadEnvelopeDoc {
    thread: GroupThreadSummaryDoc,
}

#[derive(Debug, Serialize, ToSchema)]
struct GroupThreadListResponseDoc {
    threads: Vec<GroupThreadSummaryDoc>,
}

#[derive(Debug, Serialize, ToSchema)]
struct GroupInviteRecordDoc {
    token: String,
    thread_id: Uuid,
    created_by: Uuid,
    expires_at: String,
    max_uses: Option<i32>,
    used_count: i32,
    created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
struct GroupInviteResponseDoc {
    invite: GroupInviteRecordDoc,
}

#[derive(Debug, Serialize, ToSchema)]
struct MessageRecordDoc {
    id: Uuid,
    channel_id: Uuid,
    author_id: Uuid,
    content: String,
    created_at: String,
    edited_at: Option<String>,
    deleted_at: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
struct ThreadMessageRecordDoc {
    id: Uuid,
    thread_id: Uuid,
    author_id: Uuid,
    content: String,
    created_at: String,
    edited_at: Option<String>,
    deleted_at: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
struct ChannelMessageEnvelopeDoc {
    message: MessageRecordDoc,
}

#[derive(Debug, Serialize, ToSchema)]
struct ThreadMessageEnvelopeDoc {
    message: ThreadMessageRecordDoc,
}

#[derive(Debug, Serialize, ToSchema)]
struct ChannelMessageListResponseDoc {
    messages: Vec<MessageRecordDoc>,
    next_cursor: Option<Uuid>,
}

#[derive(Debug, Serialize, ToSchema)]
struct ThreadMessageListResponseDoc {
    messages: Vec<ThreadMessageRecordDoc>,
    next_cursor: Option<Uuid>,
}

#[derive(Debug, Serialize, ToSchema)]
struct ChannelReadStateDoc {
    user_id: Uuid,
    channel_id: Uuid,
    last_read_message_id: Option<Uuid>,
    updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
struct ThreadReadStateDoc {
    user_id: Uuid,
    thread_id: Uuid,
    last_read_message_id: Option<Uuid>,
    updated_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
struct ChannelReadResponseDoc {
    read: ChannelReadStateDoc,
}

#[derive(Debug, Serialize, ToSchema)]
struct ThreadReadResponseDoc {
    read: ThreadReadStateDoc,
}

#[derive(Debug, Serialize, ToSchema)]
struct ApiErrorResponseDoc {
    error: ApiErrorPayloadDoc,
}

#[derive(Debug, Serialize, ToSchema)]
struct ApiErrorPayloadDoc {
    code: String,
    message: String,
}

#[utoipa::path(get, path = "/health", tag = "System", responses((status = 200, description = "Service health", body = HealthResponseDoc)))]
fn health_endpoint() {}

#[utoipa::path(get, path = "/openapi.json", tag = "System", responses((status = 200, description = "OpenAPI specification")))]
fn openapi_endpoint() {}

#[utoipa::path(get, path = "/metrics", tag = "System", responses((status = 200, description = "Prometheus metrics payload", content_type = "text/plain")))]
fn metrics_endpoint() {}

#[utoipa::path(get, path = "/ws", tag = "System", responses((status = 101, description = "WebSocket upgrade")))]
fn ws_endpoint() {}

#[utoipa::path(post, path = "/api/auth/register", tag = "Auth", request_body = RegisterRequest, responses((status = 200, description = "User registered", body = RegisterResponseDoc), (status = 400, description = "Validation error", body = ApiErrorResponseDoc), (status = 409, description = "Conflict", body = ApiErrorResponseDoc)))]
fn auth_register_endpoint() {}

#[utoipa::path(post, path = "/api/auth/login", tag = "Auth", request_body = LoginRequest, responses((status = 200, description = "Access and refresh tokens issued", body = TokenPairDoc), (status = 401, description = "Unauthorized", body = ApiErrorResponseDoc), (status = 429, description = "Rate limited", body = ApiErrorResponseDoc)))]
fn auth_login_endpoint() {}

#[utoipa::path(post, path = "/api/auth/refresh", tag = "Auth", request_body = RefreshRequest, responses((status = 200, description = "Token pair rotated", body = TokenPairDoc), (status = 401, description = "Unauthorized", body = ApiErrorResponseDoc)))]
fn auth_refresh_endpoint() {}

#[utoipa::path(post, path = "/api/auth/logout", tag = "Auth", request_body = LogoutRequest, responses((status = 200, description = "Session revoked", body = MessageResponse), (status = 400, description = "Validation error", body = ApiErrorResponseDoc)))]
fn auth_logout_endpoint() {}

#[utoipa::path(get, path = "/api/auth/me", tag = "Auth", responses((status = 200, description = "Authenticated user profile", body = MeResponseDoc), (status = 401, description = "Unauthorized", body = ApiErrorResponseDoc)))]
fn auth_me_endpoint() {}

#[utoipa::path(get, path = "/api/friends", tag = "Friendship", responses((status = 200, description = "Friend list", body = FriendsListResponseDoc), (status = 401, description = "Unauthorized", body = ApiErrorResponseDoc)))]
fn friends_list_endpoint() {}

#[utoipa::path(post, path = "/api/friends/requests", tag = "Friendship", request_body = CreateFriendRequest, responses((status = 200, description = "Friend request created", body = FriendRequestCreateResponseDoc), (status = 400, description = "Validation error", body = ApiErrorResponseDoc), (status = 404, description = "Not found", body = ApiErrorResponseDoc)))]
fn friends_create_request_endpoint() {}

#[utoipa::path(get, path = "/api/friends/requests", tag = "Friendship", params(("inbox" = Option<String>, Query, description = "pending|outbox")), responses((status = 200, description = "Friend request list", body = FriendRequestListResponseDoc), (status = 400, description = "Validation error", body = ApiErrorResponseDoc)))]
fn friends_list_requests_endpoint() {}

#[utoipa::path(post, path = "/api/friends/requests/{from_user_id}/accept", tag = "Friendship", params(("from_user_id" = Uuid, Path, description = "Sender user id")), responses((status = 200, description = "Friend request accepted", body = FriendAcceptResponseDoc), (status = 404, description = "Not found", body = ApiErrorResponseDoc)))]
fn friends_accept_request_endpoint() {}

#[utoipa::path(post, path = "/api/friends/requests/{from_user_id}/reject", tag = "Friendship", params(("from_user_id" = Uuid, Path, description = "Sender user id")), responses((status = 200, description = "Friend request rejected", body = MessageResponse), (status = 404, description = "Not found", body = ApiErrorResponseDoc)))]
fn friends_reject_request_endpoint() {}

#[utoipa::path(get, path = "/api/guilds", tag = "Guilds", responses((status = 200, description = "Guild list for actor", body = GuildListResponseDoc), (status = 401, description = "Unauthorized", body = ApiErrorResponseDoc)))]
fn guilds_list_endpoint() {}

#[utoipa::path(post, path = "/api/guilds", tag = "Guilds", request_body = CreateGuildRequest, responses((status = 200, description = "Guild created", body = GuildEnvelopeDoc), (status = 400, description = "Validation error", body = ApiErrorResponseDoc)))]
fn guilds_create_endpoint() {}

#[utoipa::path(get, path = "/api/guilds/{guild_id}", tag = "Guilds", params(("guild_id" = Uuid, Path, description = "Guild id")), responses((status = 200, description = "Guild detail", body = GuildEnvelopeDoc), (status = 404, description = "Not found", body = ApiErrorResponseDoc)))]
fn guilds_get_endpoint() {}

#[utoipa::path(post, path = "/api/guilds/{guild_id}/join", tag = "Guilds", params(("guild_id" = Uuid, Path, description = "Guild id")), responses((status = 200, description = "Guild joined", body = MessageResponse)))]
fn guilds_join_endpoint() {}

#[utoipa::path(post, path = "/api/guilds/{guild_id}/leave", tag = "Guilds", params(("guild_id" = Uuid, Path, description = "Guild id")), responses((status = 200, description = "Guild left", body = MessageResponse)))]
fn guilds_leave_endpoint() {}

#[utoipa::path(post, path = "/api/guilds/{guild_id}/invites", tag = "Guilds", params(("guild_id" = Uuid, Path, description = "Guild id")), request_body = CreateGuildInviteRequest, responses((status = 200, description = "Guild invite created", body = GuildInviteResponseDoc)))]
fn guilds_create_invite_endpoint() {}

#[utoipa::path(post, path = "/api/guild-invites/{token}/join", tag = "Guilds", params(("token" = String, Path, description = "Guild invite token")), responses((status = 200, description = "Guild invite consumed", body = GuildInviteJoinResponseDoc), (status = 404, description = "Not found", body = ApiErrorResponseDoc)))]
fn guild_invites_join_endpoint() {}

#[utoipa::path(get, path = "/api/guilds/{guild_id}/channels", tag = "Guilds", params(("guild_id" = Uuid, Path, description = "Guild id")), responses((status = 200, description = "Channel list", body = ChannelListResponseDoc)))]
fn guild_channels_list_endpoint() {}

#[utoipa::path(post, path = "/api/guilds/{guild_id}/channels", tag = "Guilds", params(("guild_id" = Uuid, Path, description = "Guild id")), request_body = CreateChannelRequest, responses((status = 200, description = "Channel created", body = ChannelEnvelopeDoc)))]
fn guild_channels_create_endpoint() {}

#[utoipa::path(get, path = "/api/channels/{channel_id}", tag = "Guilds", params(("channel_id" = Uuid, Path, description = "Channel id")), responses((status = 200, description = "Channel detail", body = ChannelEnvelopeDoc), (status = 404, description = "Not found", body = ApiErrorResponseDoc)))]
fn channels_get_endpoint() {}

#[utoipa::path(get, path = "/api/direct-threads", tag = "Threads", params(("inbox" = Option<String>, Query, description = "pending")), responses((status = 200, description = "Direct thread list", body = DirectThreadListResponseDoc), (status = 400, description = "Validation error", body = ApiErrorResponseDoc)))]
fn direct_threads_list_endpoint() {}

#[utoipa::path(post, path = "/api/direct-threads", tag = "Threads", request_body = CreateDirectThreadRequest, responses((status = 200, description = "Direct thread created or fetched", body = DirectThreadCreateResponseDoc), (status = 400, description = "Validation error", body = ApiErrorResponseDoc)))]
fn direct_threads_create_endpoint() {}

#[utoipa::path(get, path = "/api/direct-threads/{thread_id}", tag = "Threads", params(("thread_id" = Uuid, Path, description = "Thread id")), responses((status = 200, description = "Direct thread detail", body = DirectThreadEnvelopeDoc), (status = 404, description = "Not found", body = ApiErrorResponseDoc)))]
fn direct_threads_get_endpoint() {}

#[utoipa::path(post, path = "/api/direct-threads/{thread_id}/accept", tag = "Threads", params(("thread_id" = Uuid, Path, description = "Thread id")), responses((status = 200, description = "Direct thread accepted", body = DirectThreadEnvelopeDoc), (status = 404, description = "Not found", body = ApiErrorResponseDoc)))]
fn direct_threads_accept_endpoint() {}

#[utoipa::path(post, path = "/api/direct-threads/{thread_id}/reject", tag = "Threads", params(("thread_id" = Uuid, Path, description = "Thread id")), responses((status = 200, description = "Direct thread rejected", body = MessageResponse), (status = 404, description = "Not found", body = ApiErrorResponseDoc)))]
fn direct_threads_reject_endpoint() {}

#[utoipa::path(get, path = "/api/group-threads", tag = "Threads", responses((status = 200, description = "Group thread list", body = GroupThreadListResponseDoc)))]
fn group_threads_list_endpoint() {}

#[utoipa::path(post, path = "/api/group-threads", tag = "Threads", request_body = CreateGroupThreadRequest, responses((status = 200, description = "Group thread created", body = GroupThreadEnvelopeDoc), (status = 400, description = "Validation error", body = ApiErrorResponseDoc)))]
fn group_threads_create_endpoint() {}

#[utoipa::path(get, path = "/api/group-threads/{thread_id}", tag = "Threads", params(("thread_id" = Uuid, Path, description = "Group thread id")), responses((status = 200, description = "Group thread detail", body = GroupThreadEnvelopeDoc), (status = 404, description = "Not found", body = ApiErrorResponseDoc)))]
fn group_threads_get_endpoint() {}

#[utoipa::path(post, path = "/api/group-threads/{thread_id}/members", tag = "Threads", params(("thread_id" = Uuid, Path, description = "Group thread id")), request_body = AddGroupMemberRequest, responses((status = 200, description = "Group member added", body = MessageResponse), (status = 404, description = "Not found", body = ApiErrorResponseDoc)))]
fn group_threads_add_member_endpoint() {}

#[utoipa::path(post, path = "/api/group-threads/{thread_id}/leave", tag = "Threads", params(("thread_id" = Uuid, Path, description = "Group thread id")), responses((status = 200, description = "Group left", body = MessageResponse), (status = 404, description = "Not found", body = ApiErrorResponseDoc)))]
fn group_threads_leave_endpoint() {}

#[utoipa::path(post, path = "/api/group-threads/{thread_id}/invites", tag = "Threads", params(("thread_id" = Uuid, Path, description = "Group thread id")), request_body = CreateGroupInviteRequest, responses((status = 200, description = "Group invite created", body = GroupInviteResponseDoc), (status = 404, description = "Not found", body = ApiErrorResponseDoc)))]
fn group_threads_create_invite_endpoint() {}

#[utoipa::path(post, path = "/api/group-invites/{token}/join", tag = "Threads", params(("token" = String, Path, description = "Group invite token")), responses((status = 200, description = "Group invite consumed", body = GroupThreadEnvelopeDoc), (status = 404, description = "Not found", body = ApiErrorResponseDoc)))]
fn group_invites_join_endpoint() {}

#[utoipa::path(get, path = "/api/threads/{thread_id}/messages", tag = "Messages", params(("thread_id" = Uuid, Path, description = "Thread id"), ("before" = Option<Uuid>, Query, description = "Message id cursor"), ("limit" = Option<i64>, Query, description = "Page size")), responses((status = 200, description = "Thread message list", body = ThreadMessageListResponseDoc), (status = 400, description = "Validation error", body = ApiErrorResponseDoc)))]
fn thread_messages_list_endpoint() {}

#[utoipa::path(post, path = "/api/threads/{thread_id}/messages", tag = "Messages", params(("thread_id" = Uuid, Path, description = "Thread id")), request_body = CreateThreadMessageRequest, responses((status = 200, description = "Thread message created", body = ThreadMessageEnvelopeDoc), (status = 429, description = "Rate limited", body = ApiErrorResponseDoc)))]
fn thread_messages_create_endpoint() {}

#[utoipa::path(put, path = "/api/threads/{thread_id}/read", tag = "Threads", params(("thread_id" = Uuid, Path, description = "Thread id")), request_body = MarkThreadReadRequest, responses((status = 200, description = "Thread read marker updated", body = ThreadReadResponseDoc), (status = 404, description = "Not found", body = ApiErrorResponseDoc)))]
fn thread_read_mark_endpoint() {}

#[utoipa::path(patch, path = "/api/thread-messages/{message_id}", tag = "Messages", params(("message_id" = Uuid, Path, description = "Thread message id")), request_body = UpdateThreadMessageRequest, responses((status = 200, description = "Thread message updated", body = ThreadMessageEnvelopeDoc), (status = 404, description = "Not found", body = ApiErrorResponseDoc)))]
fn thread_message_update_endpoint() {}

#[utoipa::path(delete, path = "/api/thread-messages/{message_id}", tag = "Messages", params(("message_id" = Uuid, Path, description = "Thread message id")), responses((status = 200, description = "Thread message deleted", body = MessageResponse), (status = 404, description = "Not found", body = ApiErrorResponseDoc)))]
fn thread_message_delete_endpoint() {}

#[utoipa::path(get, path = "/api/channels/{channel_id}/messages", tag = "Messages", params(("channel_id" = Uuid, Path, description = "Channel id"), ("before" = Option<Uuid>, Query, description = "Message id cursor"), ("limit" = Option<i64>, Query, description = "Page size")), responses((status = 200, description = "Channel message list", body = ChannelMessageListResponseDoc), (status = 400, description = "Validation error", body = ApiErrorResponseDoc)))]
fn channel_messages_list_endpoint() {}

#[utoipa::path(post, path = "/api/channels/{channel_id}/messages", tag = "Messages", params(("channel_id" = Uuid, Path, description = "Channel id")), request_body = CreateMessageRequest, responses((status = 200, description = "Channel message created", body = ChannelMessageEnvelopeDoc), (status = 429, description = "Rate limited", body = ApiErrorResponseDoc)))]
fn channel_messages_create_endpoint() {}

#[utoipa::path(put, path = "/api/channels/{channel_id}/read", tag = "Guilds", params(("channel_id" = Uuid, Path, description = "Channel id")), request_body = MarkChannelReadRequest, responses((status = 200, description = "Channel read marker updated", body = ChannelReadResponseDoc), (status = 404, description = "Not found", body = ApiErrorResponseDoc)))]
fn channel_read_mark_endpoint() {}

#[utoipa::path(patch, path = "/api/messages/{message_id}", tag = "Messages", params(("message_id" = Uuid, Path, description = "Message id")), request_body = UpdateMessageRequest, responses((status = 200, description = "Channel message updated", body = ChannelMessageEnvelopeDoc), (status = 404, description = "Not found", body = ApiErrorResponseDoc)))]
fn channel_message_update_endpoint() {}

#[utoipa::path(delete, path = "/api/messages/{message_id}", tag = "Messages", params(("message_id" = Uuid, Path, description = "Message id")), responses((status = 200, description = "Channel message deleted", body = MessageResponse), (status = 404, description = "Not found", body = ApiErrorResponseDoc)))]
fn channel_message_delete_endpoint() {}
