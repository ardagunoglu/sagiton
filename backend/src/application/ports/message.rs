use crate::domain::message::{ChannelReadState, MessageRecord};
use crate::shared::error::AppResult;
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait MessageRepository: Send + Sync {
    /// Persists a channel message authored by user.
    async fn create_message(
        &self,
        channel_id: Uuid,
        author_id: Uuid,
        content: &str,
    ) -> AppResult<MessageRecord>;

    /// Lists messages in descending order with stable cursor based pagination.
    async fn list_messages(
        &self,
        channel_id: Uuid,
        before_message_id: Option<Uuid>,
        limit: i64,
    ) -> AppResult<Vec<MessageRecord>>;

    /// Returns existing message by id regardless of soft-delete state.
    async fn find_message_by_id(&self, message_id: Uuid) -> AppResult<Option<MessageRecord>>;

    /// Updates message content and edited_at timestamp.
    async fn update_message_content(
        &self,
        message_id: Uuid,
        content: &str,
    ) -> AppResult<Option<MessageRecord>>;

    /// Soft deletes message and returns resulting state.
    async fn soft_delete_message(&self, message_id: Uuid) -> AppResult<Option<MessageRecord>>;

    /// Persists read marker for channel member.
    async fn mark_channel_read(
        &self,
        channel_id: Uuid,
        user_id: Uuid,
        last_read_message_id: Option<Uuid>,
    ) -> AppResult<ChannelReadState>;
}

#[async_trait]
pub trait ChannelPermissionRepository: Send + Sync {
    /// Resolves effective channel permissions for user.
    async fn resolve_channel_permissions(
        &self,
        user_id: Uuid,
        channel_id: Uuid,
    ) -> AppResult<Option<i64>>;
}
