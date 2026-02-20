use crate::domain::friendship::{FriendRequestRecord, FriendRequestSummary, FriendSummary};
use crate::shared::error::AppResult;
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait FriendshipRepository: Send + Sync {
    /// Creates outgoing friend request for two users.
    async fn create_friend_request(
        &self,
        from_user_id: Uuid,
        to_user_id: Uuid,
    ) -> AppResult<FriendRequestRecord>;

    /// Accepts incoming friend request and creates friendship.
    async fn accept_friend_request(
        &self,
        from_user_id: Uuid,
        to_user_id: Uuid,
    ) -> AppResult<Option<FriendSummary>>;

    /// Rejects incoming friend request.
    async fn reject_friend_request(&self, from_user_id: Uuid, to_user_id: Uuid) -> AppResult<bool>;

    /// Lists incoming pending friend requests for user.
    async fn list_pending_inbox_requests(
        &self,
        user_id: Uuid,
    ) -> AppResult<Vec<FriendRequestSummary>>;

    /// Lists outgoing pending friend requests for user.
    async fn list_pending_outbox_requests(
        &self,
        user_id: Uuid,
    ) -> AppResult<Vec<FriendRequestSummary>>;

    /// Lists accepted friends for the user.
    async fn list_friends(&self, user_id: Uuid) -> AppResult<Vec<FriendSummary>>;
}
