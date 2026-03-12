use crate::bootstrap::app_state::AppState;
use crate::interfaces::http::handlers::auth::{login, logout, me, refresh, register};
use crate::interfaces::http::handlers::conversation::{
    accept_direct_thread, add_group_member, create_direct_thread, create_group_invite,
    create_group_thread, create_thread_message, delete_thread_message, get_direct_thread,
    get_group_thread, join_group_invite, leave_group_thread, list_direct_threads,
    list_group_threads, list_thread_messages, mark_thread_read, reject_direct_thread,
    update_thread_message,
};
use crate::interfaces::http::handlers::friendship::{
    accept_friend_request, create_friend_request, list_friend_requests, list_friends,
    reject_friend_request,
};
use crate::interfaces::http::handlers::guild_channel::{
    create_channel, create_guild, create_guild_invite, get_channel, get_guild, join_guild,
    join_guild_invite, leave_guild, list_channels, list_guilds,
};
use crate::interfaces::http::handlers::health::health_check;
use crate::interfaces::http::handlers::message::{
    create_message, delete_message, list_messages, mark_channel_read, update_message,
};
use crate::interfaces::http::handlers::metrics::metrics;
use crate::interfaces::http::middleware::metrics::track_metrics;
use crate::interfaces::http::openapi::ApiDoc;
use crate::interfaces::ws::gateway::ws_handler;
use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::Request,
    middleware,
    routing::{get, patch, post, put},
};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub fn build_router(state: AppState) -> Router {
    let docs_enabled =
        state.config().app_env.eq_ignore_ascii_case("development") && state.config().docs_enabled;

    let mut router = Router::new()
        .route("/health", get(health_check))
        .route("/metrics", get(metrics))
        .route("/ws", get(ws_handler))
        .route("/api/auth/register", post(register))
        .route("/api/auth/login", post(login))
        .route("/api/auth/refresh", post(refresh))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
        .route("/api/friends", get(list_friends))
        .route(
            "/api/friends/requests",
            post(create_friend_request).get(list_friend_requests),
        )
        .route(
            "/api/friends/requests/{from_user_id}/accept",
            post(accept_friend_request),
        )
        .route(
            "/api/friends/requests/{from_user_id}/reject",
            post(reject_friend_request),
        )
        .route("/api/guilds", post(create_guild).get(list_guilds))
        .route("/api/guilds/{guild_id}", get(get_guild))
        .route("/api/guilds/{guild_id}/join", post(join_guild))
        .route("/api/guilds/{guild_id}/leave", post(leave_guild))
        .route("/api/guilds/{guild_id}/invites", post(create_guild_invite))
        .route("/api/guild-invites/{token}/join", post(join_guild_invite))
        .route(
            "/api/guilds/{guild_id}/channels",
            post(create_channel).get(list_channels),
        )
        .route("/api/channels/{channel_id}", get(get_channel))
        .route(
            "/api/direct-threads",
            post(create_direct_thread).get(list_direct_threads),
        )
        .route("/api/direct-threads/{thread_id}", get(get_direct_thread))
        .route(
            "/api/direct-threads/{thread_id}/accept",
            post(accept_direct_thread),
        )
        .route(
            "/api/direct-threads/{thread_id}/reject",
            post(reject_direct_thread),
        )
        .route(
            "/api/group-threads",
            post(create_group_thread).get(list_group_threads),
        )
        .route("/api/group-threads/{thread_id}", get(get_group_thread))
        .route(
            "/api/group-threads/{thread_id}/members",
            post(add_group_member),
        )
        .route(
            "/api/group-threads/{thread_id}/leave",
            post(leave_group_thread),
        )
        .route(
            "/api/group-threads/{thread_id}/invites",
            post(create_group_invite),
        )
        .route("/api/group-invites/{token}/join", post(join_group_invite))
        .route(
            "/api/threads/{thread_id}/messages",
            post(create_thread_message).get(list_thread_messages),
        )
        .route("/api/threads/{thread_id}/read", put(mark_thread_read))
        .route(
            "/api/thread-messages/{message_id}",
            patch(update_thread_message).delete(delete_thread_message),
        )
        .route(
            "/api/channels/{channel_id}/messages",
            post(create_message).get(list_messages),
        )
        .route("/api/channels/{channel_id}/read", put(mark_channel_read))
        .route(
            "/api/messages/{message_id}",
            patch(update_message).delete(delete_message),
        )
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &Request<_>| {
                let request_id = request
                    .headers()
                    .get("x-request-id")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("missing");

                tracing::info_span!(
                    "http_request",
                    method = %request.method(),
                    uri = %request.uri(),
                    request_id = %request_id
                )
            }),
        )
        .layer(middleware::from_fn(track_metrics))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(DefaultBodyLimit::max(state.config().http_body_limit_bytes))
        .with_state(state.clone());

    if docs_enabled {
        router = router.merge(SwaggerUi::new("/docs").url("/openapi.json", ApiDoc::openapi()));
    }

    router
}
