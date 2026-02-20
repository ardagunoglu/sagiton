use crate::application::ports::realtime::{RateLimiter, RealtimePublisher};
use crate::application::use_cases::auth::AuthUseCase;
use crate::application::use_cases::conversation::ConversationUseCase;
use crate::application::use_cases::friendship::FriendshipUseCase;
use crate::application::use_cases::guild_channel::GuildChannelUseCase;
use crate::application::use_cases::message::MessageUseCase;
use crate::bootstrap::config::AppConfig;
use crate::infrastructure::db::repositories::auth_repository::SqlxAuthRepository;
use crate::infrastructure::db::repositories::conversation_repository::SqlxConversationRepository;
use crate::infrastructure::db::repositories::friendship_repository::SqlxFriendshipRepository;
use crate::infrastructure::db::repositories::guild_channel_repository::SqlxGuildChannelRepository;
use crate::infrastructure::db::repositories::message_repository::SqlxMessageRepository;
use crate::infrastructure::db::sqlx::pool::{create_pg_pool, run_migrations};
use crate::infrastructure::jwt::access_token::JwtAdapter;
use crate::infrastructure::jwt::refresh_token::RefreshTokenAdapter;
use crate::infrastructure::password::argon2_password::Argon2PasswordAdapter;
use crate::infrastructure::redis::service::RedisRealtimeService;
use crate::interfaces::ws::gateway::GatewayHub;
use crate::interfaces::ws::protocol::{PresenceStatus, ServerEvent};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    config: AppConfig,
    db_pool: PgPool,
    auth_use_case: AuthUseCase,
    conversation_use_case: ConversationUseCase,
    friendship_use_case: FriendshipUseCase,
    guild_channel_use_case: GuildChannelUseCase,
    message_use_case: MessageUseCase,
    gateway_hub: GatewayHub,
    redis_realtime: Arc<RedisRealtimeService>,
    shutdown_token: CancellationToken,
    redis_listener_task: Mutex<Option<JoinHandle<()>>>,
}

impl Drop for AppStateInner {
    fn drop(&mut self) {
        self.shutdown_token.cancel();
    }
}

impl AppState {
    /// Initializes application shared state and runtime adapters.
    ///
    /// # Parameters
    /// - `config`: Runtime configuration loaded from environment.
    pub async fn new(config: AppConfig) -> anyhow::Result<Self> {
        crate::bootstrap::metrics::init_prometheus_recorder()?;

        let db_pool = create_pg_pool(&config.database_url, config.db_max_connections).await?;
        run_migrations(&db_pool).await?;

        let auth_repo = Arc::new(SqlxAuthRepository::new(db_pool.clone()));
        let auth_use_case = AuthUseCase::new(
            auth_repo.clone(),
            auth_repo,
            Arc::new(Argon2PasswordAdapter::new()),
            Arc::new(JwtAdapter::new(&config.jwt_secret)),
            Arc::new(RefreshTokenAdapter::new(&config.refresh_token_pepper)?),
        );

        let guild_channel_repo = Arc::new(SqlxGuildChannelRepository::new(db_pool.clone()));
        let guild_channel_use_case = GuildChannelUseCase::new(guild_channel_repo.clone());

        let message_repo = Arc::new(SqlxMessageRepository::new(db_pool.clone()));
        let message_use_case = MessageUseCase::new(message_repo, guild_channel_repo);

        let conversation_repo = Arc::new(SqlxConversationRepository::new(db_pool.clone()));
        let conversation_use_case = ConversationUseCase::new(conversation_repo);

        let friendship_repo = Arc::new(SqlxFriendshipRepository::new(db_pool.clone()));
        let friendship_use_case = FriendshipUseCase::new(friendship_repo);

        let gateway_hub = GatewayHub::new();
        let redis_realtime = Arc::new(RedisRealtimeService::new(&config.redis_url)?);
        let shutdown_token = CancellationToken::new();

        let redis_listener_task = spawn_pubsub_listener(
            redis_realtime.clone(),
            gateway_hub.clone(),
            shutdown_token.child_token(),
        );

        Ok(Self {
            inner: Arc::new(AppStateInner {
                config,
                db_pool,
                auth_use_case,
                conversation_use_case,
                friendship_use_case,
                guild_channel_use_case,
                message_use_case,
                gateway_hub,
                redis_realtime,
                shutdown_token,
                redis_listener_task: Mutex::new(Some(redis_listener_task)),
            }),
        })
    }

    pub fn config(&self) -> &AppConfig {
        &self.inner.config
    }

    pub fn auth_use_case(&self) -> &AuthUseCase {
        &self.inner.auth_use_case
    }

    pub fn guild_channel_use_case(&self) -> &GuildChannelUseCase {
        &self.inner.guild_channel_use_case
    }

    pub fn conversation_use_case(&self) -> &ConversationUseCase {
        &self.inner.conversation_use_case
    }

    pub fn friendship_use_case(&self) -> &FriendshipUseCase {
        &self.inner.friendship_use_case
    }

    pub fn message_use_case(&self) -> &MessageUseCase {
        &self.inner.message_use_case
    }

    pub fn gateway_hub(&self) -> &GatewayHub {
        &self.inner.gateway_hub
    }

    pub fn shutdown_token(&self) -> CancellationToken {
        self.inner.shutdown_token.clone()
    }

    /// Publishes channel event locally and via Redis fanout.
    ///
    /// # Parameters
    /// - `channel_id`: Channel scope for event fanout.
    /// - `event`: Server event payload.
    pub async fn publish_channel_event(&self, channel_id: Uuid, event: ServerEvent) {
        self.inner
            .gateway_hub
            .broadcast_to_channel(channel_id, event.clone())
            .await;

        if let Err(err) = self
            .inner
            .redis_realtime
            .publish_channel_event(channel_id, &event)
            .await
        {
            tracing::warn!("redis channel publish failed: {err}");
        }
    }

    /// Publishes thread event locally and via Redis fanout.
    ///
    /// # Parameters
    /// - `thread_id`: Thread scope for event fanout.
    /// - `event`: Server event payload.
    pub async fn publish_thread_event(&self, thread_id: Uuid, event: ServerEvent) {
        self.inner
            .gateway_hub
            .broadcast_to_thread(thread_id, event.clone())
            .await;

        if let Err(err) = self
            .inner
            .redis_realtime
            .publish_thread_event(thread_id, &event)
            .await
        {
            tracing::warn!("redis thread publish failed: {err}");
        }
    }

    /// Publishes user-targeted event locally and via Redis fanout.
    ///
    /// # Parameters
    /// - `user_id`: Target user id.
    /// - `event`: Server event payload.
    pub async fn publish_user_event(&self, user_id: Uuid, event: ServerEvent) {
        self.inner
            .gateway_hub
            .broadcast_to_user(user_id, event.clone())
            .await;

        if let Err(err) = self
            .inner
            .redis_realtime
            .publish_user_event(user_id, &event)
            .await
        {
            tracing::warn!("redis user publish failed: {err}");
        }
    }

    /// Publishes global presence event locally and via Redis.
    ///
    /// # Parameters
    /// - `event`: Presence update server event payload.
    pub async fn publish_presence_event(&self, event: ServerEvent) {
        self.inner.gateway_hub.broadcast_global(event.clone()).await;

        if let Err(err) = self.inner.redis_realtime.publish_presence(&event).await {
            tracing::warn!("redis presence publish failed: {err}");
        }
    }

    /// Persists presence state key with TTL.
    ///
    /// # Parameters
    /// - `user_id`: User id whose presence is updated.
    /// - `status`: Presence status used for redis value.
    /// - `ttl_secs`: TTL in seconds.
    pub async fn set_presence_state(&self, user_id: Uuid, status: PresenceStatus, ttl_secs: u64) {
        match status {
            PresenceStatus::Online => {
                self.inner
                    .redis_realtime
                    .set_presence(user_id, "ONLINE", ttl_secs)
                    .await;
            }
            PresenceStatus::Offline => {
                self.inner.redis_realtime.clear_presence(user_id).await;
            }
        }
    }

    /// Applies redis-backed rate limit check.
    ///
    /// # Parameters
    /// - `key`: Unique key representing limited action.
    /// - `limit`: Max action count allowed in window.
    /// - `window_secs`: Rolling window size in seconds.
    pub async fn allow_rate_limit(&self, key: &str, limit: i64, window_secs: u64) -> bool {
        match self
            .inner
            .redis_realtime
            .allow(key, limit, window_secs)
            .await
        {
            Ok(v) => {
                if !v {
                    let scope = key.split(':').take(2).collect::<Vec<_>>().join(":");
                    crate::bootstrap::metrics::increment_rate_limit_denied(&scope);
                }
                v
            }
            Err(err) => {
                tracing::warn!("rate limit check failed, allowing request: {err}");
                true
            }
        }
    }

    #[allow(dead_code)]
    pub fn db_pool(&self) -> &PgPool {
        &self.inner.db_pool
    }

    /// Cancels long-running runtime tasks and waits listener completion.
    pub async fn shutdown(&self) {
        self.inner.shutdown_token.cancel();
        self.inner.gateway_hub.shutdown_all().await;

        let mut handle_lock = self.inner.redis_listener_task.lock().await;
        if let Some(handle) = handle_lock.take()
            && let Err(err) = handle.await
            && !err.is_cancelled()
        {
            tracing::warn!("redis listener join failed: {err}");
        }
    }
}

fn spawn_pubsub_listener(
    redis: Arc<RedisRealtimeService>,
    hub: GatewayHub,
    shutdown: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if let Err(err) = redis.run_pubsub_listener(hub, shutdown).await {
            tracing::error!("redis pubsub listener stopped: {err}");
        }
    })
}
