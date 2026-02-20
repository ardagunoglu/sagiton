use crate::application::ports::friendship::FriendshipRepository;
use crate::domain::friendship::{FriendRequestRecord, FriendRequestSummary, FriendSummary};
use crate::shared::error::{AppError, AppResult};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct FriendshipUseCase {
    repo: Arc<dyn FriendshipRepository>,
}

pub struct CreateFriendRequestCommand {
    pub from_user_id: Uuid,
    pub to_user_id: Uuid,
}

pub struct RespondFriendRequestCommand {
    pub user_id: Uuid,
    pub from_user_id: Uuid,
}

pub enum FriendRequestInboxFilter {
    Pending,
    Outbox,
}

impl FriendshipUseCase {
    /// Builds friendship use-case with repository dependency.
    ///
    /// # Parameters
    /// - `repo`: Repository that persists friendship state.
    pub fn new(repo: Arc<dyn FriendshipRepository>) -> Self {
        Self { repo }
    }

    /// Sends friend request to another user.
    ///
    /// # Parameters
    /// - `cmd`: Friend request creation command.
    pub async fn create_friend_request(
        &self,
        cmd: CreateFriendRequestCommand,
    ) -> AppResult<FriendRequestRecord> {
        if cmd.from_user_id == cmd.to_user_id {
            return Err(AppError::validation(
                "cannot send friend request to yourself",
            ));
        }

        self.repo
            .create_friend_request(cmd.from_user_id, cmd.to_user_id)
            .await
    }

    /// Accepts incoming friend request and returns friend summary.
    ///
    /// # Parameters
    /// - `cmd`: Friend request response command.
    pub async fn accept_friend_request(
        &self,
        cmd: RespondFriendRequestCommand,
    ) -> AppResult<FriendSummary> {
        self.repo
            .accept_friend_request(cmd.from_user_id, cmd.user_id)
            .await?
            .ok_or_else(|| AppError::not_found("friend request not found"))
    }

    /// Rejects incoming friend request.
    ///
    /// # Parameters
    /// - `cmd`: Friend request response command.
    pub async fn reject_friend_request(&self, cmd: RespondFriendRequestCommand) -> AppResult<()> {
        let rejected = self
            .repo
            .reject_friend_request(cmd.from_user_id, cmd.user_id)
            .await?;

        if !rejected {
            return Err(AppError::not_found("friend request not found"));
        }

        Ok(())
    }

    /// Lists pending friend requests for inbox or outbox.
    ///
    /// # Parameters
    /// - `user_id`: Authenticated user id.
    /// - `filter`: Pending inbox or outbox selector.
    pub async fn list_friend_requests(
        &self,
        user_id: Uuid,
        filter: FriendRequestInboxFilter,
    ) -> AppResult<Vec<FriendRequestSummary>> {
        match filter {
            FriendRequestInboxFilter::Pending => {
                self.repo.list_pending_inbox_requests(user_id).await
            }
            FriendRequestInboxFilter::Outbox => {
                self.repo.list_pending_outbox_requests(user_id).await
            }
        }
    }

    /// Lists all accepted friends for user.
    ///
    /// # Parameters
    /// - `user_id`: Authenticated user id.
    pub async fn list_friends(&self, user_id: Uuid) -> AppResult<Vec<FriendSummary>> {
        self.repo.list_friends(user_id).await
    }
}
