use crate::domain::channel::ChannelRecord;
use crate::domain::guild::{GuildInviteJoinResult, GuildInviteRecord, GuildSummary};
use crate::shared::error::AppResult;
use async_trait::async_trait;
use time::OffsetDateTime;
use uuid::Uuid;

#[async_trait]
pub trait GuildChannelRepository: Send + Sync {
    /// Creates a guild and automatically adds owner as a member.
    async fn create_guild(&self, owner_id: Uuid, name: &str) -> AppResult<GuildSummary>;

    /// Lists guilds where the user has membership.
    async fn list_guilds_for_user(&self, user_id: Uuid) -> AppResult<Vec<GuildSummary>>;

    /// Returns guild summary if user is a member.
    async fn find_guild_for_member(
        &self,
        guild_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<Option<GuildSummary>>;

    /// Checks if guild exists by id.
    async fn guild_exists(&self, guild_id: Uuid) -> AppResult<bool>;

    /// Adds user as guild member.
    async fn add_member(&self, guild_id: Uuid, user_id: Uuid) -> AppResult<()>;

    /// Removes user membership from guild.
    async fn remove_member(&self, guild_id: Uuid, user_id: Uuid) -> AppResult<bool>;

    /// Returns true if user is guild owner.
    async fn is_guild_owner(&self, guild_id: Uuid, user_id: Uuid) -> AppResult<bool>;

    /// Creates text channel in guild.
    async fn create_channel(&self, guild_id: Uuid, name: &str) -> AppResult<ChannelRecord>;

    /// Lists channels from guild visible to member.
    async fn list_channels_for_member(
        &self,
        guild_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<Vec<ChannelRecord>>;

    /// Returns channel details if user is member of channel's guild.
    async fn find_channel_for_member(
        &self,
        channel_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<Option<ChannelRecord>>;

    /// Creates guild invite token.
    async fn create_guild_invite(
        &self,
        guild_id: Uuid,
        created_by: Uuid,
        token: &str,
        expires_at: OffsetDateTime,
        max_uses: Option<i32>,
    ) -> AppResult<GuildInviteRecord>;

    /// Joins guild membership through invite token.
    async fn join_guild_via_invite(
        &self,
        token: &str,
        user_id: Uuid,
    ) -> AppResult<GuildInviteJoinResult>;
}
