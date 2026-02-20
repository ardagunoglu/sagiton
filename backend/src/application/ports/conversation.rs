use crate::domain::conversation::{
    DirectThreadSummary, DirectThreadTransition, DirectThreadUpsertResult, GroupInviteRecord,
    GroupThreadSummary, ThreadAccessRecord,
};
use crate::domain::message::{ThreadMessageRecord, ThreadReadState};
use crate::shared::error::AppResult;
use async_trait::async_trait;
use time::OffsetDateTime;
use uuid::Uuid;

#[async_trait]
pub trait ConversationRepository: Send + Sync {
    /// Creates or returns direct thread between two users.
    async fn create_or_get_direct_thread(
        &self,
        user_id: Uuid,
        peer_user_id: Uuid,
    ) -> AppResult<DirectThreadUpsertResult>;

    /// Lists direct threads for user.
    async fn list_direct_threads(&self, user_id: Uuid) -> AppResult<Vec<DirectThreadSummary>>;

    /// Lists pending direct thread requests for inbox.
    async fn list_pending_direct_inbox(
        &self,
        receiver_user_id: Uuid,
    ) -> AppResult<Vec<DirectThreadSummary>>;

    /// Returns direct thread detail when user is participant.
    async fn find_direct_thread_for_member(
        &self,
        thread_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<Option<DirectThreadSummary>>;

    /// Accepts a pending direct thread request.
    async fn accept_direct_thread_request(
        &self,
        thread_id: Uuid,
        receiver_user_id: Uuid,
    ) -> AppResult<Option<DirectThreadTransition>>;

    /// Rejects a pending direct thread request.
    async fn reject_direct_thread_request(
        &self,
        thread_id: Uuid,
        receiver_user_id: Uuid,
    ) -> AppResult<Option<DirectThreadTransition>>;

    /// Creates group thread and initial membership set.
    async fn create_group_thread(
        &self,
        owner_id: Uuid,
        name: &str,
        member_user_ids: &[Uuid],
    ) -> AppResult<GroupThreadSummary>;

    /// Lists group threads for member.
    async fn list_group_threads(&self, user_id: Uuid) -> AppResult<Vec<GroupThreadSummary>>;

    /// Returns group thread detail when user is member.
    async fn find_group_thread_for_member(
        &self,
        thread_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<Option<GroupThreadSummary>>;

    /// Adds user to existing group thread.
    async fn add_group_member(&self, thread_id: Uuid, user_id: Uuid) -> AppResult<()>;

    /// Removes membership from group thread.
    async fn remove_group_member(&self, thread_id: Uuid, user_id: Uuid) -> AppResult<bool>;

    /// Returns true when user owns the group thread.
    async fn is_group_owner(&self, thread_id: Uuid, user_id: Uuid) -> AppResult<bool>;

    /// Returns true when user is any thread member.
    async fn is_thread_member(&self, thread_id: Uuid, user_id: Uuid) -> AppResult<bool>;

    /// Returns thread access metadata for thread member.
    async fn find_thread_access(
        &self,
        thread_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<Option<ThreadAccessRecord>>;

    /// Persists thread message.
    async fn create_thread_message(
        &self,
        thread_id: Uuid,
        author_id: Uuid,
        content: &str,
    ) -> AppResult<ThreadMessageRecord>;

    /// Lists thread messages with cursor pagination.
    async fn list_thread_messages(
        &self,
        thread_id: Uuid,
        before_message_id: Option<Uuid>,
        limit: i64,
    ) -> AppResult<Vec<ThreadMessageRecord>>;

    /// Finds thread message by id.
    async fn find_thread_message_by_id(
        &self,
        message_id: Uuid,
    ) -> AppResult<Option<ThreadMessageRecord>>;

    /// Updates thread message content.
    async fn update_thread_message_content(
        &self,
        message_id: Uuid,
        content: &str,
    ) -> AppResult<Option<ThreadMessageRecord>>;

    /// Soft deletes thread message.
    async fn soft_delete_thread_message(
        &self,
        message_id: Uuid,
    ) -> AppResult<Option<ThreadMessageRecord>>;

    /// Creates group invite token.
    async fn create_group_invite(
        &self,
        thread_id: Uuid,
        created_by: Uuid,
        token: &str,
        expires_at: OffsetDateTime,
        max_uses: Option<i32>,
    ) -> AppResult<GroupInviteRecord>;

    /// Joins group thread via invite token.
    async fn join_group_via_invite(
        &self,
        token: &str,
        user_id: Uuid,
    ) -> AppResult<GroupThreadSummary>;

    /// Persists read marker for thread member.
    async fn mark_thread_read(
        &self,
        thread_id: Uuid,
        user_id: Uuid,
        last_read_message_id: Option<Uuid>,
    ) -> AppResult<ThreadReadState>;
}
