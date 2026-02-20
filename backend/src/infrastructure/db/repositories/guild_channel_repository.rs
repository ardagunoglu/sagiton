use crate::application::ports::guild_channel::GuildChannelRepository;
use crate::application::ports::message::ChannelPermissionRepository;
use crate::domain::channel::ChannelRecord;
use crate::domain::guild::{GuildInviteJoinResult, GuildInviteRecord, GuildSummary};
use crate::domain::permission::{SEND_MESSAGES, VIEW_CHANNEL};
use crate::shared::error::{AppError, AppResult};
use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone)]
pub struct SqlxGuildChannelRepository {
    pool: PgPool,
}

#[derive(sqlx::FromRow)]
struct JoinGuildInviteRow {
    guild_id: Uuid,
    guild_name: String,
    guild_owner_id: Uuid,
    guild_created_at: OffsetDateTime,
    expires_at: OffsetDateTime,
    max_uses: Option<i32>,
    uses_count: i32,
    revoked_at: Option<OffsetDateTime>,
}

impl SqlxGuildChannelRepository {
    /// Creates repository instance from an existing pool.
    ///
    /// # Parameters
    /// - `pool`: PostgreSQL pool shared by the application.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl GuildChannelRepository for SqlxGuildChannelRepository {
    async fn create_guild(&self, owner_id: Uuid, name: &str) -> AppResult<GuildSummary> {
        let mut tx = self.pool.begin().await.map_err(AppError::from)?;

        let guild = sqlx::query_as::<_, GuildSummary>(
            r#"
            INSERT INTO guilds (id, name, owner_id)
            VALUES ($1, $2, $3)
            RETURNING id, name, owner_id, created_at
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(name)
        .bind(owner_id)
        .fetch_one(tx.as_mut())
        .await
        .map_err(AppError::from)?;

        sqlx::query(
            r#"
            INSERT INTO guild_members (guild_id, user_id)
            VALUES ($1, $2)
            "#,
        )
        .bind(guild.id)
        .bind(owner_id)
        .execute(tx.as_mut())
        .await
        .map_err(AppError::from)?;

        let default_role_id = create_default_role(&mut tx, guild.id).await?;
        attach_default_role_to_member(&mut tx, guild.id, owner_id, default_role_id).await?;

        tx.commit().await.map_err(AppError::from)?;
        Ok(guild)
    }

    async fn list_guilds_for_user(&self, user_id: Uuid) -> AppResult<Vec<GuildSummary>> {
        let rows = sqlx::query_as::<_, GuildSummary>(
            r#"
            SELECT g.id, g.name, g.owner_id, g.created_at
            FROM guilds g
            INNER JOIN guild_members gm ON gm.guild_id = g.id
            WHERE gm.user_id = $1
            ORDER BY g.created_at ASC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(rows)
    }

    async fn find_guild_for_member(
        &self,
        guild_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<Option<GuildSummary>> {
        let row = sqlx::query_as::<_, GuildSummary>(
            r#"
            SELECT g.id, g.name, g.owner_id, g.created_at
            FROM guilds g
            INNER JOIN guild_members gm ON gm.guild_id = g.id
            WHERE g.id = $1 AND gm.user_id = $2
            LIMIT 1
            "#,
        )
        .bind(guild_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(row)
    }

    async fn guild_exists(&self, guild_id: Uuid) -> AppResult<bool> {
        let exists = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(SELECT 1 FROM guilds WHERE id = $1)
            "#,
        )
        .bind(guild_id)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(exists)
    }

    async fn add_member(&self, guild_id: Uuid, user_id: Uuid) -> AppResult<()> {
        let mut tx = self.pool.begin().await.map_err(AppError::from)?;

        let result = sqlx::query(
            r#"
            INSERT INTO guild_members (guild_id, user_id)
            VALUES ($1, $2)
            "#,
        )
        .bind(guild_id)
        .bind(user_id)
        .execute(tx.as_mut())
        .await;

        match result {
            Ok(_) => {}
            Err(err) => {
                if is_unique_violation(&err) {
                    return Err(AppError::conflict("user is already a guild member"));
                }
                return Err(AppError::from(err));
            }
        }

        let default_role_id = default_role_id(&mut tx, guild_id).await?;
        attach_default_role_to_member(&mut tx, guild_id, user_id, default_role_id).await?;

        tx.commit().await.map_err(AppError::from)?;
        Ok(())
    }

    async fn remove_member(&self, guild_id: Uuid, user_id: Uuid) -> AppResult<bool> {
        let result = sqlx::query(
            r#"
            DELETE FROM guild_members
            WHERE guild_id = $1 AND user_id = $2
            "#,
        )
        .bind(guild_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(AppError::from)?;

        sqlx::query(
            r#"
            DELETE FROM member_roles
            WHERE guild_id = $1 AND user_id = $2
            "#,
        )
        .bind(guild_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(result.rows_affected() > 0)
    }

    async fn is_guild_owner(&self, guild_id: Uuid, user_id: Uuid) -> AppResult<bool> {
        let is_owner = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(
              SELECT 1
              FROM guilds
              WHERE id = $1 AND owner_id = $2
            )
            "#,
        )
        .bind(guild_id)
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(is_owner)
    }

    async fn create_channel(&self, guild_id: Uuid, name: &str) -> AppResult<ChannelRecord> {
        let mut tx = self.pool.begin().await.map_err(AppError::from)?;

        let position = next_channel_position(&mut tx, guild_id).await?;

        let result = sqlx::query_as::<_, ChannelRecord>(
            r#"
            INSERT INTO channels (id, guild_id, name, type, position)
            VALUES ($1, $2, $3, 'TEXT', $4)
            RETURNING id, guild_id, name, type, position, created_at
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(guild_id)
        .bind(name)
        .bind(position)
        .fetch_one(tx.as_mut())
        .await;

        let channel = match result {
            Ok(value) => value,
            Err(err) => {
                if is_unique_violation(&err) {
                    return Err(AppError::conflict(
                        "channel name already exists in this guild",
                    ));
                }
                return Err(AppError::from(err));
            }
        };

        tx.commit().await.map_err(AppError::from)?;
        Ok(channel)
    }

    async fn list_channels_for_member(
        &self,
        guild_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<Vec<ChannelRecord>> {
        let rows = sqlx::query_as::<_, ChannelRecord>(
            r#"
            SELECT
                c.id,
                c.guild_id,
                c.name,
                c.type,
                c.position,
                c.created_at,
                EXISTS(
                    SELECT 1
                    FROM messages m
                    WHERE m.channel_id = c.id
                      AND m.deleted_at IS NULL
                      AND (
                        cr.last_read_message_id IS NULL
                        OR (m.created_at, m.id) > (
                            COALESCE(mr.created_at, '-infinity'::timestamptz),
                            COALESCE(mr.id, '00000000-0000-0000-0000-000000000000'::uuid)
                        )
                      )
                ) AS has_unread
            FROM channels c
            INNER JOIN guild_members gm ON gm.guild_id = c.guild_id
            LEFT JOIN channel_reads cr
                ON cr.channel_id = c.id
               AND cr.user_id = $2
            LEFT JOIN messages mr
                ON mr.id = cr.last_read_message_id
            WHERE c.guild_id = $1 AND gm.user_id = $2
            ORDER BY c.position ASC, c.created_at ASC
            "#,
        )
        .bind(guild_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(rows)
    }

    async fn find_channel_for_member(
        &self,
        channel_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<Option<ChannelRecord>> {
        let row = sqlx::query_as::<_, ChannelRecord>(
            r#"
            SELECT
                c.id,
                c.guild_id,
                c.name,
                c.type,
                c.position,
                c.created_at,
                EXISTS(
                    SELECT 1
                    FROM messages m
                    WHERE m.channel_id = c.id
                      AND m.deleted_at IS NULL
                      AND (
                        cr.last_read_message_id IS NULL
                        OR (m.created_at, m.id) > (
                            COALESCE(mr.created_at, '-infinity'::timestamptz),
                            COALESCE(mr.id, '00000000-0000-0000-0000-000000000000'::uuid)
                        )
                      )
                ) AS has_unread
            FROM channels c
            INNER JOIN guild_members gm ON gm.guild_id = c.guild_id
            LEFT JOIN channel_reads cr
                ON cr.channel_id = c.id
               AND cr.user_id = $2
            LEFT JOIN messages mr
                ON mr.id = cr.last_read_message_id
            WHERE c.id = $1 AND gm.user_id = $2
            LIMIT 1
            "#,
        )
        .bind(channel_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(row)
    }

    async fn create_guild_invite(
        &self,
        guild_id: Uuid,
        created_by: Uuid,
        token: &str,
        expires_at: OffsetDateTime,
        max_uses: Option<i32>,
    ) -> AppResult<GuildInviteRecord> {
        let invite = sqlx::query_as::<_, GuildInviteRecord>(
            r#"
            INSERT INTO guild_invites (token, guild_id, created_by, expires_at, max_uses)
            SELECT $1, gm.guild_id, $3, $4, $5
            FROM guild_members gm
            WHERE gm.guild_id = $2
              AND gm.user_id = $3
            RETURNING token, guild_id, created_by, expires_at, max_uses, uses_count, created_at
            "#,
        )
        .bind(token)
        .bind(guild_id)
        .bind(created_by)
        .bind(expires_at)
        .bind(max_uses)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)?;

        invite.ok_or_else(|| AppError::not_found("guild not found"))
    }

    async fn join_guild_via_invite(
        &self,
        token: &str,
        user_id: Uuid,
    ) -> AppResult<GuildInviteJoinResult> {
        let mut tx = self.pool.begin().await.map_err(AppError::from)?;

        let invite = sqlx::query_as::<_, JoinGuildInviteRow>(
            r#"
            SELECT
                gi.guild_id,
                g.name AS guild_name,
                g.owner_id AS guild_owner_id,
                g.created_at AS guild_created_at,
                gi.expires_at,
                gi.max_uses,
                gi.uses_count,
                gi.revoked_at
            FROM guild_invites gi
            INNER JOIN guilds g ON g.id = gi.guild_id
            WHERE gi.token = $1
            FOR UPDATE
            "#,
        )
        .bind(token)
        .fetch_optional(tx.as_mut())
        .await
        .map_err(AppError::from)?;

        let Some(invite) = invite else {
            return Err(AppError::not_found("invite not found"));
        };

        if invite.revoked_at.is_some() {
            return Err(AppError::not_found("invite not found"));
        }

        let already_member = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM guild_members
                WHERE guild_id = $1 AND user_id = $2
            )
            "#,
        )
        .bind(invite.guild_id)
        .bind(user_id)
        .fetch_one(tx.as_mut())
        .await
        .map_err(AppError::from)?;

        let guild = GuildSummary {
            id: invite.guild_id,
            name: invite.guild_name,
            owner_id: invite.guild_owner_id,
            created_at: invite.guild_created_at,
        };

        if already_member {
            tx.commit().await.map_err(AppError::from)?;
            return Ok(GuildInviteJoinResult {
                guild,
                joined: false,
            });
        }

        if invite.expires_at < OffsetDateTime::now_utc() {
            return Err(AppError::validation("invite token is expired"));
        }

        if let Some(max_uses) = invite.max_uses
            && invite.uses_count >= max_uses
        {
            return Err(AppError::validation("invite token usage limit exceeded"));
        }

        let insert_member_result = sqlx::query(
            r#"
            INSERT INTO guild_members (guild_id, user_id)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(invite.guild_id)
        .bind(user_id)
        .execute(tx.as_mut())
        .await
        .map_err(AppError::from)?;

        if insert_member_result.rows_affected() > 0 {
            let role_id = default_role_id(&mut tx, invite.guild_id).await?;
            attach_default_role_to_member(&mut tx, invite.guild_id, user_id, role_id).await?;

            sqlx::query(
                r#"
                UPDATE guild_invites
                SET uses_count = uses_count + 1
                WHERE token = $1
                "#,
            )
            .bind(token)
            .execute(tx.as_mut())
            .await
            .map_err(AppError::from)?;

            tx.commit().await.map_err(AppError::from)?;
            return Ok(GuildInviteJoinResult {
                guild,
                joined: true,
            });
        }

        tx.commit().await.map_err(AppError::from)?;
        Ok(GuildInviteJoinResult {
            guild,
            joined: false,
        })
    }
}

#[async_trait]
impl ChannelPermissionRepository for SqlxGuildChannelRepository {
    async fn resolve_channel_permissions(
        &self,
        user_id: Uuid,
        channel_id: Uuid,
    ) -> AppResult<Option<i64>> {
        let guild_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT c.guild_id
            FROM channels c
            INNER JOIN guild_members gm ON gm.guild_id = c.guild_id
            WHERE c.id = $1 AND gm.user_id = $2
            LIMIT 1
            "#,
        )
        .bind(channel_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)?;

        let Some(guild_id) = guild_id else {
            return Ok(None);
        };

        let base_permissions = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(BIT_OR(r.permissions), 0)
            FROM member_roles mr
            INNER JOIN roles r ON r.id = mr.role_id
            WHERE mr.guild_id = $1 AND mr.user_id = $2
            "#,
        )
        .bind(guild_id)
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::from)?;

        let role_deny = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(BIT_OR(co.deny), 0)
            FROM channel_overrides co
            INNER JOIN member_roles mr
                ON mr.role_id = co.target_id
               AND mr.guild_id = $1
               AND mr.user_id = $2
            WHERE co.channel_id = $3
              AND co.target_type = 'ROLE'
            "#,
        )
        .bind(guild_id)
        .bind(user_id)
        .bind(channel_id)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::from)?;

        let role_allow = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(BIT_OR(co.allow), 0)
            FROM channel_overrides co
            INNER JOIN member_roles mr
                ON mr.role_id = co.target_id
               AND mr.guild_id = $1
               AND mr.user_id = $2
            WHERE co.channel_id = $3
              AND co.target_type = 'ROLE'
            "#,
        )
        .bind(guild_id)
        .bind(user_id)
        .bind(channel_id)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::from)?;

        let member_deny = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(BIT_OR(co.deny), 0)
            FROM channel_overrides co
            WHERE co.channel_id = $1
              AND co.target_type = 'MEMBER'
              AND co.target_id = $2
            "#,
        )
        .bind(channel_id)
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::from)?;

        let member_allow = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(BIT_OR(co.allow), 0)
            FROM channel_overrides co
            WHERE co.channel_id = $1
              AND co.target_type = 'MEMBER'
              AND co.target_id = $2
            "#,
        )
        .bind(channel_id)
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::from)?;

        let role_adjusted = (base_permissions & !role_deny) | role_allow;
        let member_adjusted = (role_adjusted & !member_deny) | member_allow;

        Ok(Some(member_adjusted))
    }
}

async fn create_default_role(
    tx: &mut Transaction<'_, Postgres>,
    guild_id: Uuid,
) -> AppResult<Uuid> {
    let role_id = Uuid::new_v4();
    let default_permissions = VIEW_CHANNEL | SEND_MESSAGES;

    sqlx::query(
        r#"
        INSERT INTO roles (id, guild_id, name, permissions, position, is_default)
        VALUES ($1, $2, '@everyone', $3, 0, TRUE)
        "#,
    )
    .bind(role_id)
    .bind(guild_id)
    .bind(default_permissions)
    .execute(tx.as_mut())
    .await
    .map_err(AppError::from)?;

    Ok(role_id)
}

async fn default_role_id(tx: &mut Transaction<'_, Postgres>, guild_id: Uuid) -> AppResult<Uuid> {
    sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT id
        FROM roles
        WHERE guild_id = $1 AND is_default = TRUE
        LIMIT 1
        "#,
    )
    .bind(guild_id)
    .fetch_one(tx.as_mut())
    .await
    .map_err(AppError::from)
}

async fn attach_default_role_to_member(
    tx: &mut Transaction<'_, Postgres>,
    guild_id: Uuid,
    user_id: Uuid,
    role_id: Uuid,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO member_roles (guild_id, user_id, role_id)
        VALUES ($1, $2, $3)
        "#,
    )
    .bind(guild_id)
    .bind(user_id)
    .bind(role_id)
    .execute(tx.as_mut())
    .await
    .map_err(AppError::from)?;

    Ok(())
}

async fn next_channel_position(
    tx: &mut Transaction<'_, Postgres>,
    guild_id: Uuid,
) -> AppResult<i32> {
    let next = sqlx::query_scalar::<_, i32>(
        r#"
        SELECT COALESCE(MAX(position), -1) + 1
        FROM channels
        WHERE guild_id = $1
        "#,
    )
    .bind(guild_id)
    .fetch_one(tx.as_mut())
    .await
    .map_err(AppError::from)?;

    Ok(next)
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    matches!(
        err,
        sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("23505")
    )
}
