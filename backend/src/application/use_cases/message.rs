use crate::application::ports::message::{ChannelPermissionRepository, MessageRepository};
use crate::domain::message::{ChannelReadState, MessageDeleted, MessageRecord};
use crate::domain::permission::{PermissionBits, SEND_MESSAGES, VIEW_CHANNEL};
use crate::shared::error::{AppError, AppResult};
use std::sync::Arc;
use uuid::Uuid;

const DEFAULT_PAGE_LIMIT: i64 = 50;
const MAX_PAGE_LIMIT: i64 = 100;

#[derive(Clone)]
pub struct MessageUseCase {
    message_repo: Arc<dyn MessageRepository>,
    permission_repo: Arc<dyn ChannelPermissionRepository>,
}

pub struct CreateMessageCommand {
    pub user_id: Uuid,
    pub channel_id: Uuid,
    pub content: String,
}

pub struct ListMessagesQuery {
    pub user_id: Uuid,
    pub channel_id: Uuid,
    pub before: Option<Uuid>,
    pub limit: Option<i64>,
}

pub struct EditMessageCommand {
    pub user_id: Uuid,
    pub message_id: Uuid,
    pub content: String,
}

pub struct DeleteMessageCommand {
    pub user_id: Uuid,
    pub message_id: Uuid,
}

pub struct MarkChannelReadCommand {
    pub user_id: Uuid,
    pub channel_id: Uuid,
    pub last_read_message_id: Option<Uuid>,
}

impl MessageUseCase {
    /// Builds message use-case with repository dependencies.
    ///
    /// # Parameters
    /// - `message_repo`: Message persistence repository.
    /// - `permission_repo`: Permission resolver repository.
    pub fn new(
        message_repo: Arc<dyn MessageRepository>,
        permission_repo: Arc<dyn ChannelPermissionRepository>,
    ) -> Self {
        Self {
            message_repo,
            permission_repo,
        }
    }

    /// Creates message after channel SEND_MESSAGES authorization.
    ///
    /// # Parameters
    /// - `cmd`: Create command containing actor, channel and content.
    pub async fn create_message(&self, cmd: CreateMessageCommand) -> AppResult<MessageRecord> {
        validate_message_content(&cmd.content)?;
        let permissions = self
            .require_channel_permissions(cmd.user_id, cmd.channel_id)
            .await?;

        if !permissions.has(SEND_MESSAGES) {
            return Err(AppError::forbidden("missing SEND_MESSAGES permission"));
        }

        self.message_repo
            .create_message(cmd.channel_id, cmd.user_id, cmd.content.trim())
            .await
    }

    /// Lists non-deleted messages with stable cursor pagination.
    ///
    /// # Parameters
    /// - `query`: Query containing actor, channel and pagination cursor.
    pub async fn list_messages(&self, query: ListMessagesQuery) -> AppResult<Vec<MessageRecord>> {
        let permissions = self
            .require_channel_permissions(query.user_id, query.channel_id)
            .await?;

        if !permissions.has(VIEW_CHANNEL) {
            return Err(AppError::forbidden("missing VIEW_CHANNEL permission"));
        }

        let limit = normalize_limit(query.limit)?;
        self.message_repo
            .list_messages(query.channel_id, query.before, limit)
            .await
    }

    /// Edits author-owned message and marks edit timestamp.
    ///
    /// # Parameters
    /// - `cmd`: Edit command containing actor, message id and new content.
    pub async fn edit_message(&self, cmd: EditMessageCommand) -> AppResult<MessageRecord> {
        validate_message_content(&cmd.content)?;

        let existing = self
            .message_repo
            .find_message_by_id(cmd.message_id)
            .await?
            .ok_or_else(|| AppError::not_found("message not found"))?;

        if existing.deleted_at.is_some() {
            return Err(AppError::not_found("message not found"));
        }

        let permissions = self
            .require_channel_permissions(cmd.user_id, existing.channel_id)
            .await?;
        if !permissions.has(SEND_MESSAGES) {
            return Err(AppError::forbidden("missing SEND_MESSAGES permission"));
        }

        if existing.author_id != cmd.user_id {
            return Err(AppError::forbidden("only message author can edit message"));
        }

        self.message_repo
            .update_message_content(cmd.message_id, cmd.content.trim())
            .await?
            .ok_or_else(|| AppError::not_found("message not found"))
    }

    /// Soft deletes author-owned message.
    ///
    /// # Parameters
    /// - `cmd`: Delete command containing actor and message id.
    pub async fn delete_message(&self, cmd: DeleteMessageCommand) -> AppResult<MessageDeleted> {
        let existing = self
            .message_repo
            .find_message_by_id(cmd.message_id)
            .await?
            .ok_or_else(|| AppError::not_found("message not found"))?;

        if existing.deleted_at.is_some() {
            return Err(AppError::not_found("message not found"));
        }

        let permissions = self
            .require_channel_permissions(cmd.user_id, existing.channel_id)
            .await?;
        if !permissions.has(SEND_MESSAGES) {
            return Err(AppError::forbidden("missing SEND_MESSAGES permission"));
        }

        if existing.author_id != cmd.user_id {
            return Err(AppError::forbidden(
                "only message author can delete message",
            ));
        }

        let deleted = self
            .message_repo
            .soft_delete_message(cmd.message_id)
            .await?
            .ok_or_else(|| AppError::not_found("message not found"))?;

        Ok(MessageDeleted {
            channel_id: deleted.channel_id,
            message_id: deleted.id,
            deleted_at: deleted
                .deleted_at
                .ok_or_else(|| AppError::internal("deleted_at is missing after soft delete"))?,
        })
    }

    /// Returns effective permissions for channel.
    ///
    /// # Parameters
    /// - `user_id`: Authenticated user id.
    /// - `channel_id`: Target channel id.
    pub async fn channel_permissions(
        &self,
        user_id: Uuid,
        channel_id: Uuid,
    ) -> AppResult<PermissionBits> {
        self.require_channel_permissions(user_id, channel_id).await
    }

    /// Persists channel read marker for channel member.
    ///
    /// # Parameters
    /// - `cmd`: Read marker command with actor, channel and optional message id.
    pub async fn mark_channel_read(
        &self,
        cmd: MarkChannelReadCommand,
    ) -> AppResult<ChannelReadState> {
        let permissions = self
            .require_channel_permissions(cmd.user_id, cmd.channel_id)
            .await?;

        if !permissions.has(VIEW_CHANNEL) {
            return Err(AppError::forbidden("missing VIEW_CHANNEL permission"));
        }

        self.message_repo
            .mark_channel_read(cmd.channel_id, cmd.user_id, cmd.last_read_message_id)
            .await
    }

    async fn require_channel_permissions(
        &self,
        user_id: Uuid,
        channel_id: Uuid,
    ) -> AppResult<PermissionBits> {
        let bits = self
            .permission_repo
            .resolve_channel_permissions(user_id, channel_id)
            .await?
            .ok_or_else(|| AppError::not_found("channel not found"))?;

        Ok(PermissionBits(bits))
    }
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
