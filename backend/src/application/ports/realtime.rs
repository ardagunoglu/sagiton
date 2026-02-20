use crate::interfaces::ws::protocol::ServerEvent;
use crate::shared::error::AppResult;
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait RealtimePublisher: Send + Sync {
    /// Publishes event to channel-scoped realtime stream.
    async fn publish_channel_event(&self, channel_id: Uuid, event: &ServerEvent) -> AppResult<()>;

    /// Publishes event to thread-scoped realtime stream.
    async fn publish_thread_event(&self, thread_id: Uuid, event: &ServerEvent) -> AppResult<()>;

    /// Publishes event to a specific user's realtime stream.
    async fn publish_user_event(&self, user_id: Uuid, event: &ServerEvent) -> AppResult<()>;

    /// Publishes global presence update event.
    async fn publish_presence(&self, event: &ServerEvent) -> AppResult<()>;
}

#[async_trait]
pub trait RateLimiter: Send + Sync {
    /// Returns true when action is allowed under configured window and limit.
    async fn allow(&self, key: &str, limit: i64, window_secs: u64) -> AppResult<bool>;
}
