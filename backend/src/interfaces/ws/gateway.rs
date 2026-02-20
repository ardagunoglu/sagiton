use crate::bootstrap::app_state::AppState;
use crate::bootstrap::metrics::{increment_ws_events_sent, set_ws_connections_active};
use crate::domain::permission::{SEND_MESSAGES, VIEW_CHANNEL};
use crate::interfaces::ws::protocol::{ClientCommand, PresenceStatus, ServerEvent};
use crate::shared::error::{AppError, AppResult};
use axum::{
    extract::{Query, State, ws::Message, ws::WebSocket, ws::WebSocketUpgrade},
    http::{HeaderMap, header},
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{RwLock, mpsc};
use tracing::{Instrument, info_span};
use uuid::Uuid;

const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(70);
const PRESENCE_TTL_SECS: u64 = 90;

#[derive(Clone)]
pub struct GatewayHub {
    inner: Arc<RwLock<HubState>>,
}

struct HubState {
    connections: HashMap<Uuid, ConnectionState>,
    user_connections: HashMap<Uuid, HashSet<Uuid>>,
    channel_subscribers: HashMap<Uuid, HashSet<Uuid>>,
    thread_subscribers: HashMap<Uuid, HashSet<Uuid>>,
}

struct ConnectionState {
    user_id: Uuid,
    sender: mpsc::UnboundedSender<ServerEvent>,
    subscribed_channels: HashSet<Uuid>,
    subscribed_threads: HashSet<Uuid>,
    last_heartbeat: Instant,
}

pub struct RegisterResult {
    pub connection_id: Uuid,
    pub first_connection: bool,
}

pub struct UnregisterResult {
    pub user_id: Uuid,
    pub became_offline: bool,
}

#[derive(Debug, serde::Deserialize)]
pub struct WsAuthQuery {
    token: Option<String>,
}

impl GatewayHub {
    /// Creates in-memory websocket hub for local instance subscribers.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(HubState {
                connections: HashMap::new(),
                user_connections: HashMap::new(),
                channel_subscribers: HashMap::new(),
                thread_subscribers: HashMap::new(),
            })),
        }
    }

    /// Registers websocket connection and returns generated connection id.
    ///
    /// # Parameters
    /// - `user_id`: Authenticated user id.
    /// - `sender`: Outbound channel sender for server events.
    pub async fn register(
        &self,
        user_id: Uuid,
        sender: mpsc::UnboundedSender<ServerEvent>,
    ) -> RegisterResult {
        let connection_id = Uuid::new_v4();
        let mut state = self.inner.write().await;

        state.connections.insert(
            connection_id,
            ConnectionState {
                user_id,
                sender,
                subscribed_channels: HashSet::new(),
                subscribed_threads: HashSet::new(),
                last_heartbeat: Instant::now(),
            },
        );

        let first_connection = {
            let connections = state.user_connections.entry(user_id).or_default();
            connections.insert(connection_id);
            connections.len() == 1
        };
        set_ws_connections_active(state.connections.len());

        RegisterResult {
            connection_id,
            first_connection,
        }
    }

    /// Unregisters websocket connection and clears all channel subscriptions.
    ///
    /// # Parameters
    /// - `connection_id`: Connection id returned by `register`.
    pub async fn unregister(&self, connection_id: Uuid) -> Option<UnregisterResult> {
        let mut state = self.inner.write().await;
        let conn = state.connections.remove(&connection_id)?;

        for channel_id in &conn.subscribed_channels {
            if let Some(set) = state.channel_subscribers.get_mut(channel_id) {
                set.remove(&connection_id);
                if set.is_empty() {
                    state.channel_subscribers.remove(channel_id);
                }
            }
        }

        for thread_id in &conn.subscribed_threads {
            if let Some(set) = state.thread_subscribers.get_mut(thread_id) {
                set.remove(&connection_id);
                if set.is_empty() {
                    state.thread_subscribers.remove(thread_id);
                }
            }
        }

        let mut became_offline = false;
        if let Some(set) = state.user_connections.get_mut(&conn.user_id) {
            set.remove(&connection_id);
            if set.is_empty() {
                state.user_connections.remove(&conn.user_id);
                became_offline = true;
            }
        }
        set_ws_connections_active(state.connections.len());

        Some(UnregisterResult {
            user_id: conn.user_id,
            became_offline,
        })
    }

    /// Subscribes connection to channel-scoped events.
    ///
    /// # Parameters
    /// - `connection_id`: Registered connection id.
    /// - `channel_id`: Channel id to subscribe.
    pub async fn subscribe_channel(&self, connection_id: Uuid, channel_id: Uuid) {
        let mut state = self.inner.write().await;

        if let Some(conn) = state.connections.get_mut(&connection_id) {
            conn.subscribed_channels.insert(channel_id);
        }

        state
            .channel_subscribers
            .entry(channel_id)
            .or_default()
            .insert(connection_id);
    }

    /// Unsubscribes connection from channel-scoped events.
    ///
    /// # Parameters
    /// - `connection_id`: Registered connection id.
    /// - `channel_id`: Channel id to unsubscribe.
    pub async fn unsubscribe_channel(&self, connection_id: Uuid, channel_id: Uuid) {
        let mut state = self.inner.write().await;

        if let Some(conn) = state.connections.get_mut(&connection_id) {
            conn.subscribed_channels.remove(&channel_id);
        }

        if let Some(set) = state.channel_subscribers.get_mut(&channel_id) {
            set.remove(&connection_id);
            if set.is_empty() {
                state.channel_subscribers.remove(&channel_id);
            }
        }
    }

    /// Subscribes connection to thread-scoped events.
    ///
    /// # Parameters
    /// - `connection_id`: Registered connection id.
    /// - `thread_id`: Thread id to subscribe.
    pub async fn subscribe_thread(&self, connection_id: Uuid, thread_id: Uuid) {
        let mut state = self.inner.write().await;

        if let Some(conn) = state.connections.get_mut(&connection_id) {
            conn.subscribed_threads.insert(thread_id);
        }

        state
            .thread_subscribers
            .entry(thread_id)
            .or_default()
            .insert(connection_id);
    }

    /// Unsubscribes connection from thread-scoped events.
    ///
    /// # Parameters
    /// - `connection_id`: Registered connection id.
    /// - `thread_id`: Thread id to unsubscribe.
    pub async fn unsubscribe_thread(&self, connection_id: Uuid, thread_id: Uuid) {
        let mut state = self.inner.write().await;

        if let Some(conn) = state.connections.get_mut(&connection_id) {
            conn.subscribed_threads.remove(&thread_id);
        }

        if let Some(set) = state.thread_subscribers.get_mut(&thread_id) {
            set.remove(&connection_id);
            if set.is_empty() {
                state.thread_subscribers.remove(&thread_id);
            }
        }
    }

    /// Updates heartbeat timestamp for connection.
    ///
    /// # Parameters
    /// - `connection_id`: Registered connection id.
    pub async fn touch_heartbeat(&self, connection_id: Uuid) {
        let mut state = self.inner.write().await;
        if let Some(conn) = state.connections.get_mut(&connection_id) {
            conn.last_heartbeat = Instant::now();
        }
    }

    /// Broadcasts event to all local subscribers of channel.
    ///
    /// # Parameters
    /// - `channel_id`: Channel id used for subscription lookup.
    /// - `event`: Event payload sent to subscribed connections.
    pub async fn broadcast_to_channel(&self, channel_id: Uuid, event: ServerEvent) {
        let state = self.inner.read().await;
        let Some(subscribers) = state.channel_subscribers.get(&channel_id) else {
            return;
        };

        for connection_id in subscribers {
            if let Some(conn) = state.connections.get(connection_id) {
                send_event(conn, event.clone());
            }
        }
    }

    /// Broadcasts event to all local subscribers of thread.
    ///
    /// # Parameters
    /// - `thread_id`: Thread id used for subscription lookup.
    /// - `event`: Event payload sent to subscribed connections.
    pub async fn broadcast_to_thread(&self, thread_id: Uuid, event: ServerEvent) {
        let state = self.inner.read().await;
        let Some(subscribers) = state.thread_subscribers.get(&thread_id) else {
            return;
        };

        for connection_id in subscribers {
            if let Some(conn) = state.connections.get(connection_id) {
                send_event(conn, event.clone());
            }
        }
    }

    /// Broadcasts event to all local websocket connections.
    ///
    /// # Parameters
    /// - `event`: Event payload sent to all connected clients.
    pub async fn broadcast_global(&self, event: ServerEvent) {
        let state = self.inner.read().await;
        for conn in state.connections.values() {
            send_event(conn, event.clone());
        }
    }

    /// Broadcasts event to all local websocket connections of a specific user.
    ///
    /// # Parameters
    /// - `user_id`: Target user id.
    /// - `event`: Event payload sent to all user's active connections.
    pub async fn broadcast_to_user(&self, user_id: Uuid, event: ServerEvent) {
        let state = self.inner.read().await;
        let Some(connections) = state.user_connections.get(&user_id) else {
            return;
        };

        for connection_id in connections {
            if let Some(conn) = state.connections.get(connection_id) {
                send_event(conn, event.clone());
            }
        }
    }

    /// Sends event to a single websocket connection.
    ///
    /// # Parameters
    /// - `connection_id`: Target connection id.
    /// - `event`: Event payload for the target connection.
    pub async fn send_to_connection(&self, connection_id: Uuid, event: ServerEvent) {
        let state = self.inner.read().await;
        if let Some(conn) = state.connections.get(&connection_id) {
            send_event(conn, event);
        }
    }

    /// Drops all active websocket senders and clears subscription state.
    pub async fn shutdown_all(&self) {
        let mut state = self.inner.write().await;
        state.connections.clear();
        state.user_connections.clear();
        state.channel_subscribers.clear();
        state.thread_subscribers.clear();
        set_ws_connections_active(0);
    }
}

impl Default for GatewayHub {
    fn default() -> Self {
        Self::new()
    }
}

fn send_event(conn: &ConnectionState, event: ServerEvent) {
    let event_name = server_event_name(&event);
    if conn.sender.send(event).is_ok() {
        increment_ws_events_sent(event_name);
    }
}

fn server_event_name(event: &ServerEvent) -> &'static str {
    match event {
        ServerEvent::Ready { .. } => "READY",
        ServerEvent::HeartbeatAck { .. } => "HEARTBEAT_ACK",
        ServerEvent::ChannelSubscribeAck { .. } => "CHANNEL_SUBSCRIBE_ACK",
        ServerEvent::ThreadSubscribeAck { .. } => "THREAD_SUBSCRIBE_ACK",
        ServerEvent::PresenceUpdate { .. } => "PRESENCE_UPDATE",
        ServerEvent::MessageCreate { .. } => "MESSAGE_CREATE",
        ServerEvent::MessageUpdate { .. } => "MESSAGE_UPDATE",
        ServerEvent::MessageDelete { .. } => "MESSAGE_DELETE",
        ServerEvent::ThreadMessageCreate { .. } => "THREAD_MESSAGE_CREATE",
        ServerEvent::ThreadMessageUpdate { .. } => "THREAD_MESSAGE_UPDATE",
        ServerEvent::ThreadMessageDelete { .. } => "THREAD_MESSAGE_DELETE",
        ServerEvent::ThreadRequestReceived { .. } => "THREAD_REQUEST_RECEIVED",
        ServerEvent::ThreadAccepted { .. } => "THREAD_ACCEPTED",
        ServerEvent::Typing { .. } => "TYPING",
        ServerEvent::ThreadTyping { .. } => "THREAD_TYPING",
        ServerEvent::Error { .. } => "ERROR",
    }
}

/// Handles websocket upgrade and session bootstrap.
///
/// # Parameters
/// - `query`: Optional query token (`?token=`).
/// - `headers`: Optional authorization header fallback.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<WsAuthQuery>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let token = resolve_ws_token(query.token, &headers)?;
    let user_id = state.auth_use_case().verify_access_token(&token)?;

    Ok(ws.on_upgrade(move |socket| async move {
        if let Err(err) = handle_socket(socket, state, user_id).await {
            tracing::warn!("ws session ended with error: {err}");
        }
    }))
}

async fn handle_socket(socket: WebSocket, state: AppState, user_id: Uuid) -> AppResult<()> {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerEvent>();
    let registration = state.gateway_hub().register(user_id, tx.clone()).await;
    let connection_id = registration.connection_id;
    let ws_span = info_span!("ws_session", %connection_id, %user_id);
    let shutdown_token = state.shutdown_token();

    if registration.first_connection {
        state
            .set_presence_state(user_id, PresenceStatus::Online, PRESENCE_TTL_SECS)
            .await;
        state
            .publish_presence_event(ServerEvent::PresenceUpdate {
                user_id,
                status: PresenceStatus::Online,
            })
            .await;
    }

    let _ = tx.send(ServerEvent::Ready { user_id });

    let writer = tokio::spawn(
        async move {
            while let Some(event) = rx.recv().await {
                let payload = match serde_json::to_string(&event) {
                    Ok(value) => value,
                    Err(err) => {
                        tracing::warn!("failed to serialize ws event: {err}");
                        continue;
                    }
                };

                if sink.send(Message::Text(payload.into())).await.is_err() {
                    break;
                }
            }
        }
        .instrument(ws_span.clone()),
    );

    loop {
        let next = tokio::select! {
            _ = shutdown_token.cancelled() => {
                tracing::info!(%connection_id, "ws shutdown requested");
                break;
            }
            next = tokio::time::timeout(HEARTBEAT_TIMEOUT, stream.next()) => next,
        };
        let frame = match next {
            Ok(Some(Ok(v))) => v,
            Ok(Some(Err(_))) => break,
            Ok(None) => break,
            Err(_) => break,
        };

        match frame {
            Message::Text(payload) => {
                if payload.len() > state.config().ws_max_message_bytes {
                    state
                        .gateway_hub()
                        .send_to_connection(
                            connection_id,
                            ServerEvent::Error {
                                code: "FRAME_TOO_LARGE".to_string(),
                                message: "frame too large".to_string(),
                            },
                        )
                        .await;
                    break;
                }

                let cmd = match serde_json::from_str::<ClientCommand>(&payload) {
                    Ok(v) => v,
                    Err(_) => {
                        state
                            .gateway_hub()
                            .send_to_connection(
                                connection_id,
                                ServerEvent::Error {
                                    code: "INVALID_COMMAND".to_string(),
                                    message: "failed to parse command".to_string(),
                                },
                            )
                            .await;
                        continue;
                    }
                };

                handle_command(&state, connection_id, user_id, cmd)
                    .instrument(ws_span.clone())
                    .await;
            }
            Message::Close(_) => break,
            Message::Ping(_) | Message::Pong(_) | Message::Binary(_) => {}
        }
    }

    if let Some(unregister) = state.gateway_hub().unregister(connection_id).await
        && unregister.became_offline
    {
        state
            .set_presence_state(
                unregister.user_id,
                PresenceStatus::Offline,
                PRESENCE_TTL_SECS,
            )
            .await;
        state
            .publish_presence_event(ServerEvent::PresenceUpdate {
                user_id: unregister.user_id,
                status: PresenceStatus::Offline,
            })
            .await;
    }

    writer.abort();
    Ok(())
}

async fn handle_command(state: &AppState, connection_id: Uuid, user_id: Uuid, cmd: ClientCommand) {
    match cmd {
        ClientCommand::Identify { .. } => {
            state
                .gateway_hub()
                .send_to_connection(connection_id, ServerEvent::Ready { user_id })
                .await;
        }
        ClientCommand::Heartbeat { ts } => {
            state.gateway_hub().touch_heartbeat(connection_id).await;
            state
                .set_presence_state(user_id, PresenceStatus::Online, PRESENCE_TTL_SECS)
                .await;
            state
                .gateway_hub()
                .send_to_connection(connection_id, ServerEvent::HeartbeatAck { ts })
                .await;
        }
        ClientCommand::SubscribeChannel { channel_id } => {
            let perms = state
                .message_use_case()
                .channel_permissions(user_id, channel_id)
                .await;
            match perms {
                Ok(bits) if bits.has(VIEW_CHANNEL) => {
                    state
                        .gateway_hub()
                        .subscribe_channel(connection_id, channel_id)
                        .await;
                    state
                        .gateway_hub()
                        .send_to_connection(
                            connection_id,
                            ServerEvent::ChannelSubscribeAck { channel_id },
                        )
                        .await;
                }
                Ok(_) => {
                    state
                        .gateway_hub()
                        .send_to_connection(
                            connection_id,
                            ServerEvent::Error {
                                code: "FORBIDDEN".to_string(),
                                message: "missing VIEW_CHANNEL permission".to_string(),
                            },
                        )
                        .await;
                }
                Err(err) => {
                    state
                        .gateway_hub()
                        .send_to_connection(
                            connection_id,
                            ServerEvent::Error {
                                code: "SUBSCRIBE_FAILED".to_string(),
                                message: err.to_string(),
                            },
                        )
                        .await;
                }
            }
        }
        ClientCommand::UnsubscribeChannel { channel_id } => {
            state
                .gateway_hub()
                .unsubscribe_channel(connection_id, channel_id)
                .await;
        }
        ClientCommand::TypingStart { channel_id } => {
            let perms = state
                .message_use_case()
                .channel_permissions(user_id, channel_id)
                .await;
            let Ok(bits) = perms else {
                return;
            };

            if !bits.has(SEND_MESSAGES) {
                return;
            }

            let rl_key = format!("rl:typing:{user_id}:{channel_id}");
            let allowed = state.allow_rate_limit(&rl_key, 8, 5).await;
            if !allowed {
                return;
            }

            let event = ServerEvent::Typing {
                channel_id,
                user_id,
            };
            state.publish_channel_event(channel_id, event).await;
        }
        ClientCommand::SubscribeThread { thread_id } => {
            tracing::debug!(%connection_id, %user_id, %thread_id, "ws subscribe thread");
            match state
                .conversation_use_case()
                .can_access_thread(thread_id, user_id)
                .await
            {
                Ok(true) => {
                    state
                        .gateway_hub()
                        .subscribe_thread(connection_id, thread_id)
                        .await;
                    state
                        .gateway_hub()
                        .send_to_connection(
                            connection_id,
                            ServerEvent::ThreadSubscribeAck { thread_id },
                        )
                        .await;
                }
                Ok(false) => {
                    state
                        .gateway_hub()
                        .send_to_connection(
                            connection_id,
                            ServerEvent::Error {
                                code: "FORBIDDEN".to_string(),
                                message: "thread not accessible".to_string(),
                            },
                        )
                        .await;
                }
                Err(err) => {
                    state
                        .gateway_hub()
                        .send_to_connection(
                            connection_id,
                            ServerEvent::Error {
                                code: "SUBSCRIBE_FAILED".to_string(),
                                message: err.to_string(),
                            },
                        )
                        .await;
                }
            }
        }
        ClientCommand::UnsubscribeThread { thread_id } => {
            tracing::debug!(%connection_id, %user_id, %thread_id, "ws unsubscribe thread");
            state
                .gateway_hub()
                .unsubscribe_thread(connection_id, thread_id)
                .await;
        }
        ClientCommand::ThreadTypingStart { thread_id } => {
            tracing::debug!(%connection_id, %user_id, %thread_id, "ws thread typing");
            let can_access = match state
                .conversation_use_case()
                .can_access_thread(thread_id, user_id)
                .await
            {
                Ok(v) => v,
                Err(_) => return,
            };

            if !can_access {
                return;
            }

            let rl_key = format!("rl:typing:thread:{user_id}:{thread_id}");
            let allowed = state.allow_rate_limit(&rl_key, 8, 5).await;
            if !allowed {
                return;
            }

            let event = ServerEvent::ThreadTyping { thread_id, user_id };
            state.publish_thread_event(thread_id, event).await;
        }
    }
}

fn resolve_ws_token(token_query: Option<String>, headers: &HeaderMap) -> AppResult<String> {
    if let Some(token) = token_query {
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    let header_value = headers
        .get(header::AUTHORIZATION)
        .ok_or_else(|| AppError::unauthorized("authorization token is required"))?
        .to_str()
        .map_err(|_| AppError::unauthorized("authorization header is invalid"))?;

    let token = header_value
        .strip_prefix("Bearer ")
        .ok_or_else(|| AppError::unauthorized("bearer token is required"))?;

    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err(AppError::unauthorized("bearer token is required"));
    }

    Ok(trimmed.to_string())
}
