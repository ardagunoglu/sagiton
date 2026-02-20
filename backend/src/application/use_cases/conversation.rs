use crate::application::ports::conversation::ConversationRepository;
use crate::domain::conversation::{
    DirectThreadSummary, DirectThreadTransition, DirectThreadUpsertResult, GroupInviteRecord,
    GroupThreadSummary,
};
use crate::domain::message::{ThreadMessageDeleted, ThreadMessageRecord, ThreadReadState};
use crate::shared::error::{AppError, AppResult};
use std::collections::BTreeSet;
use std::sync::Arc;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

const DEFAULT_PAGE_LIMIT: i64 = 50;
const MAX_PAGE_LIMIT: i64 = 100;
const DEFAULT_INVITE_EXPIRES_SECS: u64 = 7 * 24 * 60 * 60;
const MAX_INVITE_EXPIRES_SECS: u64 = 30 * 24 * 60 * 60;

#[derive(Clone)]
pub struct ConversationUseCase {
    repo: Arc<dyn ConversationRepository>,
}

pub enum DirectInboxFilter {
    All,
    Pending,
}

pub struct CreateDirectThreadCommand {
    pub user_id: Uuid,
    pub peer_user_id: Uuid,
}

pub struct AcceptDirectThreadCommand {
    pub user_id: Uuid,
    pub thread_id: Uuid,
}

pub struct RejectDirectThreadCommand {
    pub user_id: Uuid,
    pub thread_id: Uuid,
}

pub struct CreateGroupThreadCommand {
    pub owner_id: Uuid,
    pub name: String,
    pub member_user_ids: Vec<Uuid>,
}

pub struct AddGroupMemberCommand {
    pub actor_user_id: Uuid,
    pub thread_id: Uuid,
    pub user_id: Uuid,
}

pub struct LeaveGroupCommand {
    pub user_id: Uuid,
    pub thread_id: Uuid,
}

pub struct CreateGroupInviteCommand {
    pub actor_user_id: Uuid,
    pub thread_id: Uuid,
    pub expires_in_seconds: Option<u64>,
    pub max_uses: Option<i32>,
}

pub struct JoinGroupInviteCommand {
    pub user_id: Uuid,
    pub token: String,
}

pub struct CreateThreadMessageCommand {
    pub user_id: Uuid,
    pub thread_id: Uuid,
    pub content: String,
}

pub struct ListThreadMessagesQuery {
    pub user_id: Uuid,
    pub thread_id: Uuid,
    pub before: Option<Uuid>,
    pub limit: Option<i64>,
}

pub struct EditThreadMessageCommand {
    pub user_id: Uuid,
    pub message_id: Uuid,
    pub content: String,
}

pub struct DeleteThreadMessageCommand {
    pub user_id: Uuid,
    pub message_id: Uuid,
}

pub struct MarkThreadReadCommand {
    pub user_id: Uuid,
    pub thread_id: Uuid,
    pub last_read_message_id: Option<Uuid>,
}

impl ConversationUseCase {
    /// Builds conversation use-case with repository dependency.
    ///
    /// # Parameters
    /// - `repo`: Conversation repository implementation.
    pub fn new(repo: Arc<dyn ConversationRepository>) -> Self {
        Self { repo }
    }

    /// Creates or gets direct thread between two users.
    ///
    /// # Parameters
    /// - `cmd`: Command containing actor user and peer user ids.
    pub async fn create_direct_thread(
        &self,
        cmd: CreateDirectThreadCommand,
    ) -> AppResult<DirectThreadUpsertResult> {
        if cmd.user_id == cmd.peer_user_id {
            return Err(AppError::validation(
                "cannot create direct thread with yourself",
            ));
        }

        self.repo
            .create_or_get_direct_thread(cmd.user_id, cmd.peer_user_id)
            .await
    }

    /// Lists direct threads for a user.
    ///
    /// # Parameters
    /// - `user_id`: Authenticated user id.
    /// - `filter`: Inbox filter (`All` or `Pending`).
    pub async fn list_direct_threads(
        &self,
        user_id: Uuid,
        filter: DirectInboxFilter,
    ) -> AppResult<Vec<DirectThreadSummary>> {
        match filter {
            DirectInboxFilter::All => self.repo.list_direct_threads(user_id).await,
            DirectInboxFilter::Pending => self.repo.list_pending_direct_inbox(user_id).await,
        }
    }

    /// Returns direct thread details for participant.
    ///
    /// # Parameters
    /// - `thread_id`: Direct thread id.
    /// - `user_id`: Authenticated user id.
    pub async fn get_direct_thread(
        &self,
        thread_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<DirectThreadSummary> {
        self.repo
            .find_direct_thread_for_member(thread_id, user_id)
            .await?
            .ok_or_else(|| AppError::not_found("direct thread not found"))
    }

    /// Accepts pending direct thread request as receiver.
    ///
    /// # Parameters
    /// - `cmd`: Accept command containing receiver and thread ids.
    pub async fn accept_direct_thread(
        &self,
        cmd: AcceptDirectThreadCommand,
    ) -> AppResult<DirectThreadTransition> {
        self.repo
            .accept_direct_thread_request(cmd.thread_id, cmd.user_id)
            .await?
            .ok_or_else(|| AppError::not_found("direct thread not found"))
    }

    /// Rejects pending direct thread request as receiver.
    ///
    /// # Parameters
    /// - `cmd`: Reject command containing receiver and thread ids.
    pub async fn reject_direct_thread(
        &self,
        cmd: RejectDirectThreadCommand,
    ) -> AppResult<DirectThreadTransition> {
        self.repo
            .reject_direct_thread_request(cmd.thread_id, cmd.user_id)
            .await?
            .ok_or_else(|| AppError::not_found("direct thread not found"))
    }

    /// Creates group thread and initial members.
    ///
    /// # Parameters
    /// - `cmd`: Group creation command containing owner, name and member ids.
    pub async fn create_group_thread(
        &self,
        cmd: CreateGroupThreadCommand,
    ) -> AppResult<GroupThreadSummary> {
        validate_group_name(&cmd.name)?;

        let members = normalize_member_ids(cmd.owner_id, cmd.member_user_ids)?;
        self.repo
            .create_group_thread(cmd.owner_id, cmd.name.trim(), &members)
            .await
    }

    /// Lists group threads where user is member.
    ///
    /// # Parameters
    /// - `user_id`: Authenticated user id.
    pub async fn list_group_threads(&self, user_id: Uuid) -> AppResult<Vec<GroupThreadSummary>> {
        self.repo.list_group_threads(user_id).await
    }

    /// Returns group thread details for member.
    ///
    /// # Parameters
    /// - `thread_id`: Group thread id.
    /// - `user_id`: Authenticated user id.
    pub async fn get_group_thread(
        &self,
        thread_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<GroupThreadSummary> {
        self.repo
            .find_group_thread_for_member(thread_id, user_id)
            .await?
            .ok_or_else(|| AppError::not_found("group thread not found"))
    }

    /// Adds member to group, only owner can perform operation.
    ///
    /// # Parameters
    /// - `cmd`: Add group member command.
    pub async fn add_group_member(&self, cmd: AddGroupMemberCommand) -> AppResult<()> {
        let owner = self
            .repo
            .is_group_owner(cmd.thread_id, cmd.actor_user_id)
            .await?;
        if !owner {
            return Err(AppError::forbidden("only group owner can add members"));
        }

        self.repo.add_group_member(cmd.thread_id, cmd.user_id).await
    }

    /// Removes caller membership from group.
    ///
    /// # Parameters
    /// - `cmd`: Leave group command.
    pub async fn leave_group(&self, cmd: LeaveGroupCommand) -> AppResult<()> {
        if self.repo.is_group_owner(cmd.thread_id, cmd.user_id).await? {
            return Err(AppError::validation("group owner cannot leave group"));
        }

        let removed = self
            .repo
            .remove_group_member(cmd.thread_id, cmd.user_id)
            .await?;
        if !removed {
            return Err(AppError::not_found("group membership not found"));
        }

        Ok(())
    }

    /// Creates group invite token for group owner.
    ///
    /// # Parameters
    /// - `cmd`: Group invite create command.
    pub async fn create_group_invite(
        &self,
        cmd: CreateGroupInviteCommand,
    ) -> AppResult<GroupInviteRecord> {
        let group = self
            .repo
            .find_group_thread_for_member(cmd.thread_id, cmd.actor_user_id)
            .await?;
        if group.is_none() {
            return Err(AppError::not_found("group thread not found"));
        }

        let owner = self
            .repo
            .is_group_owner(cmd.thread_id, cmd.actor_user_id)
            .await?;
        if !owner {
            return Err(AppError::forbidden("only group owner can create invite"));
        }

        validate_max_uses(cmd.max_uses)?;
        let expires_in = normalize_invite_expiry(cmd.expires_in_seconds);
        let expires_at = OffsetDateTime::now_utc() + Duration::seconds(expires_in as i64);
        let token = Uuid::new_v4().simple().to_string();

        self.repo
            .create_group_invite(
                cmd.thread_id,
                cmd.actor_user_id,
                &token,
                expires_at,
                cmd.max_uses,
            )
            .await
    }

    /// Joins group thread through invite token.
    ///
    /// # Parameters
    /// - `cmd`: Invite join command.
    pub async fn join_group_invite(
        &self,
        cmd: JoinGroupInviteCommand,
    ) -> AppResult<GroupThreadSummary> {
        let token = cmd.token.trim();
        if token.is_empty() {
            return Err(AppError::validation("invite token is required"));
        }

        self.repo.join_group_via_invite(token, cmd.user_id).await
    }

    /// Persists thread read marker for thread member.
    ///
    /// # Parameters
    /// - `cmd`: Read marker command with actor, thread and optional message id.
    pub async fn mark_thread_read(&self, cmd: MarkThreadReadCommand) -> AppResult<ThreadReadState> {
        self.require_thread_member(cmd.thread_id, cmd.user_id)
            .await?;
        self.repo
            .mark_thread_read(cmd.thread_id, cmd.user_id, cmd.last_read_message_id)
            .await
    }

    /// Creates thread message for thread participant.
    ///
    /// # Parameters
    /// - `cmd`: Thread message create command.
    pub async fn create_thread_message(
        &self,
        cmd: CreateThreadMessageCommand,
    ) -> AppResult<ThreadMessageRecord> {
        validate_message_content(&cmd.content)?;
        let access = self
            .repo
            .find_thread_access(cmd.thread_id, cmd.user_id)
            .await?
            .ok_or_else(|| AppError::not_found("thread not found"))?;

        if access.kind == "DIRECT"
            && access.direct_status == "PENDING"
            && access.requested_by != Some(cmd.user_id)
        {
            return Err(AppError::forbidden(
                "direct thread request must be accepted before receiver can send messages",
            ));
        }

        self.repo
            .create_thread_message(cmd.thread_id, cmd.user_id, cmd.content.trim())
            .await
    }

    /// Lists thread messages for thread participant.
    ///
    /// # Parameters
    /// - `query`: Thread message list query.
    pub async fn list_thread_messages(
        &self,
        query: ListThreadMessagesQuery,
    ) -> AppResult<Vec<ThreadMessageRecord>> {
        self.require_thread_member(query.thread_id, query.user_id)
            .await?;

        let limit = normalize_limit(query.limit)?;
        self.repo
            .list_thread_messages(query.thread_id, query.before, limit)
            .await
    }

    /// Edits author-owned thread message.
    ///
    /// # Parameters
    /// - `cmd`: Thread message edit command.
    pub async fn edit_thread_message(
        &self,
        cmd: EditThreadMessageCommand,
    ) -> AppResult<ThreadMessageRecord> {
        validate_message_content(&cmd.content)?;

        let existing = self
            .repo
            .find_thread_message_by_id(cmd.message_id)
            .await?
            .ok_or_else(|| AppError::not_found("thread message not found"))?;

        if existing.deleted_at.is_some() {
            return Err(AppError::not_found("thread message not found"));
        }

        self.require_thread_member(existing.thread_id, cmd.user_id)
            .await?;

        if existing.author_id != cmd.user_id {
            return Err(AppError::forbidden(
                "only message author can edit thread message",
            ));
        }

        self.repo
            .update_thread_message_content(cmd.message_id, cmd.content.trim())
            .await?
            .ok_or_else(|| AppError::not_found("thread message not found"))
    }

    /// Soft deletes author-owned thread message.
    ///
    /// # Parameters
    /// - `cmd`: Thread message delete command.
    pub async fn delete_thread_message(
        &self,
        cmd: DeleteThreadMessageCommand,
    ) -> AppResult<ThreadMessageDeleted> {
        let existing = self
            .repo
            .find_thread_message_by_id(cmd.message_id)
            .await?
            .ok_or_else(|| AppError::not_found("thread message not found"))?;

        if existing.deleted_at.is_some() {
            return Err(AppError::not_found("thread message not found"));
        }

        self.require_thread_member(existing.thread_id, cmd.user_id)
            .await?;

        if existing.author_id != cmd.user_id {
            return Err(AppError::forbidden(
                "only message author can delete thread message",
            ));
        }

        let deleted = self
            .repo
            .soft_delete_thread_message(cmd.message_id)
            .await?
            .ok_or_else(|| AppError::not_found("thread message not found"))?;

        Ok(ThreadMessageDeleted {
            thread_id: deleted.thread_id,
            message_id: deleted.id,
            deleted_at: deleted.deleted_at.ok_or_else(|| {
                AppError::internal("deleted_at is missing after thread soft delete")
            })?,
        })
    }

    /// Returns true when user is thread member.
    ///
    /// # Parameters
    /// - `thread_id`: Thread id.
    /// - `user_id`: User id.
    pub async fn can_access_thread(&self, thread_id: Uuid, user_id: Uuid) -> AppResult<bool> {
        self.repo.is_thread_member(thread_id, user_id).await
    }

    async fn require_thread_member(&self, thread_id: Uuid, user_id: Uuid) -> AppResult<()> {
        let member = self.repo.is_thread_member(thread_id, user_id).await?;
        if !member {
            return Err(AppError::not_found("thread not found"));
        }

        Ok(())
    }
}

fn validate_group_name(name: &str) -> AppResult<()> {
    let value = name.trim();
    if value.len() < 2 {
        return Err(AppError::validation(
            "group name must be at least 2 characters",
        ));
    }

    if value.len() > 80 {
        return Err(AppError::validation(
            "group name must be at most 80 characters",
        ));
    }

    Ok(())
}

fn normalize_member_ids(owner_id: Uuid, member_user_ids: Vec<Uuid>) -> AppResult<Vec<Uuid>> {
    let mut normalized = BTreeSet::new();
    for member_id in member_user_ids {
        if member_id == owner_id {
            continue;
        }
        normalized.insert(member_id);
    }

    if normalized.is_empty() {
        return Ok(Vec::new());
    }

    if normalized.len() > 30 {
        return Err(AppError::validation(
            "group member count must be at most 30",
        ));
    }

    Ok(normalized.into_iter().collect())
}

fn validate_message_content(content: &str) -> AppResult<()> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation("message content is required"));
    }

    if trimmed.len() > 4000 {
        return Err(AppError::validation(
            "message content must be at most 4000 characters",
        ));
    }

    Ok(())
}

fn normalize_limit(limit: Option<i64>) -> AppResult<i64> {
    let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT);
    if limit <= 0 {
        return Err(AppError::validation("limit must be positive"));
    }

    if limit > MAX_PAGE_LIMIT {
        return Ok(MAX_PAGE_LIMIT);
    }

    Ok(limit)
}

fn validate_max_uses(max_uses: Option<i32>) -> AppResult<()> {
    if let Some(value) = max_uses
        && value <= 0
    {
        return Err(AppError::validation("max_uses must be positive"));
    }

    Ok(())
}

fn normalize_invite_expiry(expires_in_seconds: Option<u64>) -> u64 {
    let value = expires_in_seconds.unwrap_or(DEFAULT_INVITE_EXPIRES_SECS);
    value.clamp(60, MAX_INVITE_EXPIRES_SECS)
}
