use crate::application::ports::guild_channel::GuildChannelRepository;
use crate::domain::channel::ChannelRecord;
use crate::domain::guild::{GuildInviteJoinResult, GuildInviteRecord, GuildSummary};
use crate::shared::error::{AppError, AppResult};
use std::sync::Arc;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

const DEFAULT_INVITE_EXPIRES_SECS: u64 = 7 * 24 * 60 * 60;
const MAX_INVITE_EXPIRES_SECS: u64 = 30 * 24 * 60 * 60;

#[derive(Clone)]
pub struct GuildChannelUseCase {
    repo: Arc<dyn GuildChannelRepository>,
}

pub struct CreateGuildCommand {
    pub user_id: Uuid,
    pub name: String,
}

pub struct JoinGuildCommand {
    pub user_id: Uuid,
    pub guild_id: Uuid,
}

pub struct LeaveGuildCommand {
    pub user_id: Uuid,
    pub guild_id: Uuid,
}

pub struct CreateChannelCommand {
    pub user_id: Uuid,
    pub guild_id: Uuid,
    pub name: String,
}

pub struct CreateGuildInviteCommand {
    pub actor_user_id: Uuid,
    pub guild_id: Uuid,
    pub expires_in_seconds: Option<u64>,
    pub max_uses: Option<i32>,
}

pub struct JoinGuildInviteCommand {
    pub user_id: Uuid,
    pub token: String,
}

impl GuildChannelUseCase {
    /// Builds guild/channel use-case with repository dependency.
    ///
    /// # Parameters
    /// - `repo`: Repository that persists guild and channel state.
    pub fn new(repo: Arc<dyn GuildChannelRepository>) -> Self {
        Self { repo }
    }

    /// Creates guild and owner membership.
    ///
    /// # Parameters
    /// - `cmd`: Command containing owner id and guild name.
    pub async fn create_guild(&self, cmd: CreateGuildCommand) -> AppResult<GuildSummary> {
        validate_name("guild name", &cmd.name, 2, 80)?;
        self.repo.create_guild(cmd.user_id, cmd.name.trim()).await
    }

    /// Lists guilds that the user belongs to.
    ///
    /// # Parameters
    /// - `user_id`: Authenticated user id.
    pub async fn list_guilds(&self, user_id: Uuid) -> AppResult<Vec<GuildSummary>> {
        self.repo.list_guilds_for_user(user_id).await
    }

    /// Returns guild details when user is a member.
    ///
    /// # Parameters
    /// - `guild_id`: Target guild id.
    /// - `user_id`: Authenticated user id.
    pub async fn get_guild(&self, guild_id: Uuid, user_id: Uuid) -> AppResult<GuildSummary> {
        self.repo
            .find_guild_for_member(guild_id, user_id)
            .await?
            .ok_or_else(|| AppError::not_found("guild not found"))
    }

    /// Adds user to guild membership list.
    ///
    /// # Parameters
    /// - `cmd`: Command containing user id and guild id.
    pub async fn join_guild(&self, cmd: JoinGuildCommand) -> AppResult<()> {
        if !self.repo.guild_exists(cmd.guild_id).await? {
            return Err(AppError::not_found("guild not found"));
        }

        self.repo.add_member(cmd.guild_id, cmd.user_id).await
    }

    /// Removes user from guild memberships.
    ///
    /// # Parameters
    /// - `cmd`: Command containing user id and guild id.
    pub async fn leave_guild(&self, cmd: LeaveGuildCommand) -> AppResult<()> {
        if self.repo.is_guild_owner(cmd.guild_id, cmd.user_id).await? {
            return Err(AppError::validation(
                "guild owner cannot leave their own guild",
            ));
        }

        let removed = self.repo.remove_member(cmd.guild_id, cmd.user_id).await?;
        if !removed {
            return Err(AppError::not_found("guild membership not found"));
        }

        Ok(())
    }

    /// Creates a text channel in a guild for a member.
    ///
    /// # Parameters
    /// - `cmd`: Command containing actor, guild and channel name.
    pub async fn create_channel(&self, cmd: CreateChannelCommand) -> AppResult<ChannelRecord> {
        validate_name("channel name", &cmd.name, 1, 80)?;
        self.get_guild(cmd.guild_id, cmd.user_id).await?;

        self.repo
            .create_channel(cmd.guild_id, cmd.name.trim())
            .await
    }

    /// Lists channels from guild visible to user.
    ///
    /// # Parameters
    /// - `guild_id`: Target guild id.
    /// - `user_id`: Authenticated user id.
    pub async fn list_channels(
        &self,
        guild_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<Vec<ChannelRecord>> {
        self.get_guild(guild_id, user_id).await?;
        self.repo.list_channels_for_member(guild_id, user_id).await
    }

    /// Returns channel details when user belongs to parent guild.
    ///
    /// # Parameters
    /// - `channel_id`: Target channel id.
    /// - `user_id`: Authenticated user id.
    pub async fn get_channel(&self, channel_id: Uuid, user_id: Uuid) -> AppResult<ChannelRecord> {
        self.repo
            .find_channel_for_member(channel_id, user_id)
            .await?
            .ok_or_else(|| AppError::not_found("channel not found"))
    }

    /// Creates guild invite token for guild member.
    ///
    /// # Parameters
    /// - `cmd`: Invite create command with actor, guild and optional expiry/usage cap.
    pub async fn create_guild_invite(
        &self,
        cmd: CreateGuildInviteCommand,
    ) -> AppResult<GuildInviteRecord> {
        self.get_guild(cmd.guild_id, cmd.actor_user_id).await?;
        validate_max_uses(cmd.max_uses)?;

        let expires_in = normalize_invite_expiry(cmd.expires_in_seconds);
        let expires_at = OffsetDateTime::now_utc() + Duration::seconds(expires_in as i64);
        let token = Uuid::new_v4().simple().to_string();

        self.repo
            .create_guild_invite(
                cmd.guild_id,
                cmd.actor_user_id,
                &token,
                expires_at,
                cmd.max_uses,
            )
            .await
    }

    /// Joins guild via invite token.
    ///
    /// # Parameters
    /// - `cmd`: Invite join command with actor and invite token.
    pub async fn join_guild_invite(
        &self,
        cmd: JoinGuildInviteCommand,
    ) -> AppResult<GuildInviteJoinResult> {
        let token = cmd.token.trim();
        if token.is_empty() {
            return Err(AppError::validation("invite token is required"));
        }

        self.repo.join_guild_via_invite(token, cmd.user_id).await
    }
}

fn validate_name(field_name: &str, value: &str, min: usize, max: usize) -> AppResult<()> {
    let trimmed = value.trim();
    if trimmed.len() < min {
        return Err(AppError::validation(format!(
            "{field_name} must be at least {min} characters",
        )));
    }

    if trimmed.len() > max {
        return Err(AppError::validation(format!(
            "{field_name} must be at most {max} characters",
        )));
    }

    Ok(())
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
    let secs = expires_in_seconds.unwrap_or(DEFAULT_INVITE_EXPIRES_SECS);
    secs.clamp(60, MAX_INVITE_EXPIRES_SECS)
}
