use crate::application::ports::realtime::{RateLimiter, RealtimePublisher};
use crate::bootstrap::metrics::increment_redis_reconnect;
use crate::interfaces::ws::gateway::GatewayHub;
use crate::interfaces::ws::protocol::ServerEvent;
use crate::shared::error::{AppError, AppResult};
use async_trait::async_trait;
use futures_util::StreamExt;
use rand::Rng;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

const INITIAL_BACKOFF_MS: u64 = 250;
const MAX_BACKOFF_MS: u64 = 10_000;
const MAX_JITTER_MS: u64 = 250;

#[derive(Clone)]
pub struct RedisRealtimeService {
    client: redis::Client,
    instance_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct RealtimeEnvelope {
    origin_instance_id: String,
    event: ServerEvent,
}

enum SessionExit {
    Shutdown,
    Disconnected,
}

impl RedisRealtimeService {
    /// Creates redis realtime service from connection url.
    ///
    /// # Parameters
    /// - `redis_url`: Redis URL used for async commands and pub/sub.
    pub fn new(redis_url: &str) -> AppResult<Self> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| AppError::internal(format!("invalid redis url: {e}")))?;

        Ok(Self {
            client,
            instance_id: Uuid::new_v4().to_string(),
        })
    }

    /// Starts resilient pub/sub listener and forwards events to local websocket hub.
    ///
    /// # Parameters
    /// - `hub`: Gateway hub that broadcasts to local subscribers.
    /// - `shutdown`: Cancellation token used for graceful task stop.
    pub async fn run_pubsub_listener(
        &self,
        hub: GatewayHub,
        shutdown: CancellationToken,
    ) -> AppResult<()> {
        let mut reconnect_attempt: u32 = 0;

        loop {
            if shutdown.is_cancelled() {
                info!(instance_id = %self.instance_id, "redis listener shutdown completed");
                return Ok(());
            }

            let session = self.run_pubsub_session(hub.clone(), shutdown.clone()).await;
            match session {
                Ok(SessionExit::Shutdown) => {
                    info!(instance_id = %self.instance_id, "redis listener received shutdown");
                    return Ok(());
                }
                Ok(SessionExit::Disconnected) => {
                    reconnect_attempt = 1;
                    increment_redis_reconnect();
                    let delay = reconnect_delay(reconnect_attempt);
                    warn!(
                        instance_id = %self.instance_id,
                        reconnect_attempt,
                        backoff_ms = delay.as_millis(),
                        "redis pubsub disconnected, reconnecting"
                    );
                    tokio::select! {
                        _ = shutdown.cancelled() => return Ok(()),
                        _ = tokio::time::sleep(delay) => {}
                    }
                }
                Err(err) => {
                    reconnect_attempt = reconnect_attempt.saturating_add(1);
                    increment_redis_reconnect();
                    let delay = reconnect_delay(reconnect_attempt);
                    warn!(
                        instance_id = %self.instance_id,
                        reconnect_attempt,
                        backoff_ms = delay.as_millis(),
                        error = %err,
                        "redis pubsub session failed, reconnecting"
                    );
                    tokio::select! {
                        _ = shutdown.cancelled() => return Ok(()),
                        _ = tokio::time::sleep(delay) => {}
                    }
                }
            }
        }
    }

    async fn run_pubsub_session(
        &self,
        hub: GatewayHub,
        shutdown: CancellationToken,
    ) -> AppResult<SessionExit> {
        let mut conn = self
            .client
            .get_async_pubsub()
            .await
            .map_err(|e| AppError::internal(format!("redis pubsub connect failed: {e}")))?;
        info!(instance_id = %self.instance_id, "redis pubsub connected");

        conn.psubscribe("events:channel:*")
            .await
            .map_err(|e| AppError::internal(format!("redis psubscribe failed: {e}")))?;
        conn.psubscribe("events:thread:*")
            .await
            .map_err(|e| AppError::internal(format!("redis psubscribe failed: {e}")))?;
        conn.psubscribe("events:user:*")
            .await
            .map_err(|e| AppError::internal(format!("redis psubscribe failed: {e}")))?;
        conn.subscribe("events:presence")
            .await
            .map_err(|e| AppError::internal(format!("redis subscribe failed: {e}")))?;
        info!(
            instance_id = %self.instance_id,
            "redis pubsub subscriptions established"
        );

        let mut stream = conn.on_message();
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => return Ok(SessionExit::Shutdown),
                next_message = stream.next() => {
                    let Some(msg) = next_message else {
                        return Ok(SessionExit::Disconnected);
                    };
                    let payload: String = match msg.get_payload() {
                        Ok(v) => v,
                        Err(err) => {
                            warn!("failed to decode redis payload: {err}");
                            continue;
                        }
                    };

                    let envelope = match serde_json::from_str::<RealtimeEnvelope>(&payload) {
                        Ok(v) => v,
                        Err(_) => {
                            let event: ServerEvent = match serde_json::from_str(&payload) {
                                Ok(v) => v,
                                Err(err) => {
                                    warn!("failed to decode event json from redis: {err}");
                                    continue;
                                }
                            };
                            RealtimeEnvelope {
                                origin_instance_id: String::new(),
                                event,
                            }
                        }
                    };

                    if !envelope.origin_instance_id.is_empty()
                        && envelope.origin_instance_id == self.instance_id
                    {
                        continue;
                    }

                    dispatch_event(&hub, envelope.event).await;
                }
            }
        }
    }

    /// Sets presence state with TTL for fail-safe offline detection.
    ///
    /// # Parameters
    /// - `user_id`: User id to update.
    /// - `status`: Presence status persisted as uppercase text.
    /// - `ttl_secs`: Redis TTL in seconds.
    pub async fn set_presence(&self, user_id: Uuid, status: &str, ttl_secs: u64) {
        let key = format!("presence:user:{user_id}");
        let mut conn = match self.client.get_multiplexed_async_connection().await {
            Ok(v) => v,
            Err(err) => {
                warn!("presence connection failed: {err}");
                return;
            }
        };

        let result: redis::RedisResult<()> = conn.set_ex(key, status, ttl_secs).await;
        if let Err(err) = result {
            warn!("failed to set presence: {err}");
        }
    }

    /// Deletes presence state key for user.
    ///
    /// # Parameters
    /// - `user_id`: User id whose presence will be removed.
    pub async fn clear_presence(&self, user_id: Uuid) {
        let key = format!("presence:user:{user_id}");
        let mut conn = match self.client.get_multiplexed_async_connection().await {
            Ok(v) => v,
            Err(err) => {
                warn!("presence delete connection failed: {err}");
                return;
            }
        };

        let result: redis::RedisResult<usize> = conn.del(key).await;
        if let Err(err) = result {
            warn!("failed to clear presence: {err}");
        }
    }

    async fn publish_envelope(&self, channel: &str, event: &ServerEvent) -> AppResult<()> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| AppError::internal(format!("redis connection failed: {e}")))?;

        let envelope = RealtimeEnvelope {
            origin_instance_id: self.instance_id.clone(),
            event: event.clone(),
        };
        let payload = serde_json::to_string(&envelope)
            .map_err(|e| AppError::internal(format!("event serialization failed: {e}")))?;

        conn.publish::<_, _, i64>(channel, payload)
            .await
            .map_err(|e| AppError::internal(format!("redis publish failed: {e}")))?;

        info!(
            instance_id = %self.instance_id,
            channel,
            event = event_name(event),
            "redis event published"
        );
        Ok(())
    }
}

#[async_trait]
impl RealtimePublisher for RedisRealtimeService {
    async fn publish_channel_event(&self, channel_id: Uuid, event: &ServerEvent) -> AppResult<()> {
        let channel = format!("events:channel:{channel_id}");
        self.publish_envelope(&channel, event).await
    }

    async fn publish_thread_event(&self, thread_id: Uuid, event: &ServerEvent) -> AppResult<()> {
        let channel = format!("events:thread:{thread_id}");
        self.publish_envelope(&channel, event).await
    }

    async fn publish_user_event(&self, user_id: Uuid, event: &ServerEvent) -> AppResult<()> {
        let channel = format!("events:user:{user_id}");
        self.publish_envelope(&channel, event).await
    }

    async fn publish_presence(&self, event: &ServerEvent) -> AppResult<()> {
        self.publish_envelope("events:presence", event).await
    }
}

#[async_trait]
impl RateLimiter for RedisRealtimeService {
    async fn allow(&self, key: &str, limit: i64, window_secs: u64) -> AppResult<bool> {
        let mut conn = match self.client.get_multiplexed_async_connection().await {
            Ok(v) => v,
            Err(err) => {
                warn!("redis unavailable for rate limit, allowing request: {err}");
                return Ok(true);
            }
        };

        let count: i64 = conn
            .incr(key, 1)
            .await
            .map_err(|e| AppError::internal(format!("rate limit increment failed: {e}")))?;

        if count == 1 {
            let set_expiry: redis::RedisResult<bool> = conn.expire(key, window_secs as i64).await;
            if let Err(err) = set_expiry {
                error!("failed to set rate limit expiry: {err}");
            }
        }

        Ok(count <= limit)
    }
}

async fn dispatch_event(hub: &GatewayHub, event: ServerEvent) {
    match &event {
        ServerEvent::MessageCreate { channel_id, .. }
        | ServerEvent::MessageUpdate { channel_id, .. }
        | ServerEvent::MessageDelete { channel_id, .. }
        | ServerEvent::Typing { channel_id, .. } => {
            hub.broadcast_to_channel(*channel_id, event.clone()).await;
        }
        ServerEvent::ThreadMessageCreate { thread_id, .. }
        | ServerEvent::ThreadMessageUpdate { thread_id, .. }
        | ServerEvent::ThreadMessageDelete { thread_id, .. }
        | ServerEvent::ThreadTyping { thread_id, .. } => {
            hub.broadcast_to_thread(*thread_id, event.clone()).await;
        }
        ServerEvent::ThreadRequestReceived {
            receiver_user_id, ..
        } => {
            hub.broadcast_to_user(*receiver_user_id, event.clone())
                .await;
        }
        ServerEvent::ThreadAccepted {
            requester_user_id,
            receiver_user_id,
            ..
        } => {
            hub.broadcast_to_user(*requester_user_id, event.clone())
                .await;
            if requester_user_id != receiver_user_id {
                hub.broadcast_to_user(*receiver_user_id, event.clone())
                    .await;
            }
        }
        ServerEvent::PresenceUpdate { .. } => {
            hub.broadcast_global(event.clone()).await;
        }
        _ => {}
    }
}

fn reconnect_delay(attempt: u32) -> Duration {
    let exponent = attempt.saturating_sub(1).min(7);
    let base = INITIAL_BACKOFF_MS.saturating_mul(1_u64 << exponent);
    let capped = base.min(MAX_BACKOFF_MS);
    let jitter: u64 = rand::rng().random_range(0..=MAX_JITTER_MS);
    Duration::from_millis(capped.saturating_add(jitter))
}

fn event_name(event: &ServerEvent) -> &'static str {
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
