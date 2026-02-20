use crate::application::ports::conversation::ConversationRepository;
use crate::domain::conversation::{
    DirectThreadSummary, DirectThreadTransition, DirectThreadUpsertResult, GroupInviteRecord,
    GroupThreadSummary, ThreadAccessRecord,
};
use crate::domain::message::{ThreadMessageRecord, ThreadReadState};
use crate::shared::error::{AppError, AppResult};
use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone)]
pub struct SqlxConversationRepository {
    pool: PgPool,
}

struct DirectThreadStateRow {
    id: Uuid,
    direct_status: String,
    requested_by: Option<Uuid>,
}

#[derive(sqlx::FromRow)]
struct JoinInviteRow {
    thread_id: Uuid,
    name: String,
    owner_id: Uuid,
    thread_created_at: OffsetDateTime,
    expires_at: OffsetDateTime,
    max_uses: Option<i32>,
    used_count: i32,
    revoked_at: Option<OffsetDateTime>,
}

impl SqlxConversationRepository {
    /// Creates repository instance from an existing pool.
    ///
    /// # Parameters
    /// - `pool`: PostgreSQL pool shared by the application.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ConversationRepository for SqlxConversationRepository {
    async fn create_or_get_direct_thread(
        &self,
        user_id: Uuid,
        peer_user_id: Uuid,
    ) -> AppResult<DirectThreadUpsertResult> {
        let (a, b) = sort_pair(user_id, peer_user_id);
        let mut tx = self.pool.begin().await.map_err(AppError::from)?;

        let are_friends = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM friends
                WHERE user_id_a = $1 AND user_id_b = $2
            )
            "#,
        )
        .bind(a)
        .bind(b)
        .fetch_one(tx.as_mut())
        .await
        .map_err(AppError::from)?;

        let desired_status = if are_friends { "ACTIVE" } else { "PENDING" };
        let requested_by = if are_friends { None } else { Some(user_id) };
        let accepted_at = if are_friends {
            Some(OffsetDateTime::now_utc())
        } else {
            None
        };

        let created_thread_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO threads (
                id,
                kind,
                direct_a_user_id,
                direct_b_user_id,
                direct_status,
                requested_by,
                accepted_at
            )
            VALUES ($1, 'DIRECT', $2, $3, $4, $5, $6)
            ON CONFLICT (kind, direct_a_user_id, direct_b_user_id) DO NOTHING
            RETURNING id
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(a)
        .bind(b)
        .bind(desired_status)
        .bind(requested_by)
        .bind(accepted_at)
        .fetch_optional(tx.as_mut())
        .await
        .map_err(map_fk_as_not_found)?;

        let thread_id = match created_thread_id {
            Some(id) => id,
            None => sqlx::query_scalar::<_, Uuid>(
                r#"
                    SELECT id
                    FROM threads
                    WHERE kind = 'DIRECT'
                      AND direct_a_user_id = $1
                      AND direct_b_user_id = $2
                    LIMIT 1
                    "#,
            )
            .bind(a)
            .bind(b)
            .fetch_one(tx.as_mut())
            .await
            .map_err(AppError::from)?,
        };

        ensure_direct_membership(&mut tx, thread_id, a, b).await?;
        let summary = direct_summary_for_member(&mut tx, thread_id, user_id).await?;

        tx.commit().await.map_err(AppError::from)?;
        Ok(DirectThreadUpsertResult {
            thread: summary,
            created: created_thread_id.is_some(),
        })
    }

    async fn list_direct_threads(&self, user_id: Uuid) -> AppResult<Vec<DirectThreadSummary>> {
        let rows = sqlx::query_as::<_, DirectThreadSummary>(
            r#"
            SELECT
                t.id,
                CASE
                    WHEN t.direct_a_user_id = $1 THEN t.direct_b_user_id
                    ELSE t.direct_a_user_id
                END AS peer_user_id,
                u.username AS peer_username,
                t.direct_status AS status,
                t.requested_by,
                t.accepted_at,
                t.created_at,
                EXISTS(
                    SELECT 1
                    FROM thread_messages m
                    WHERE m.thread_id = t.id
                      AND m.deleted_at IS NULL
                      AND (
                        tr.last_read_message_id IS NULL
                        OR (m.created_at, m.id) > (
                            COALESCE(trm.created_at, '-infinity'::timestamptz),
                            COALESCE(trm.id, '00000000-0000-0000-0000-000000000000'::uuid)
                        )
                      )
                ) AS has_unread
            FROM threads t
            INNER JOIN thread_members tm
                ON tm.thread_id = t.id
            INNER JOIN users u
                ON u.id = CASE
                    WHEN t.direct_a_user_id = $1 THEN t.direct_b_user_id
                    ELSE t.direct_a_user_id
                END
            LEFT JOIN thread_reads tr
                ON tr.thread_id = t.id
               AND tr.user_id = $1
            LEFT JOIN thread_messages trm
                ON trm.id = tr.last_read_message_id
            WHERE t.kind = 'DIRECT'
              AND tm.user_id = $1
            ORDER BY t.created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(rows)
    }

    async fn list_pending_direct_inbox(
        &self,
        receiver_user_id: Uuid,
    ) -> AppResult<Vec<DirectThreadSummary>> {
        let rows = sqlx::query_as::<_, DirectThreadSummary>(
            r#"
            SELECT
                t.id,
                CASE
                    WHEN t.direct_a_user_id = $1 THEN t.direct_b_user_id
                    ELSE t.direct_a_user_id
                END AS peer_user_id,
                u.username AS peer_username,
                t.direct_status AS status,
                t.requested_by,
                t.accepted_at,
                t.created_at,
                EXISTS(
                    SELECT 1
                    FROM thread_messages m
                    WHERE m.thread_id = t.id
                      AND m.deleted_at IS NULL
                      AND (
                        tr.last_read_message_id IS NULL
                        OR (m.created_at, m.id) > (
                            COALESCE(trm.created_at, '-infinity'::timestamptz),
                            COALESCE(trm.id, '00000000-0000-0000-0000-000000000000'::uuid)
                        )
                      )
                ) AS has_unread
            FROM threads t
            INNER JOIN thread_members tm
                ON tm.thread_id = t.id
            INNER JOIN users u
                ON u.id = CASE
                    WHEN t.direct_a_user_id = $1 THEN t.direct_b_user_id
                    ELSE t.direct_a_user_id
                END
            LEFT JOIN thread_reads tr
                ON tr.thread_id = t.id
               AND tr.user_id = $1
            LEFT JOIN thread_messages trm
                ON trm.id = tr.last_read_message_id
            WHERE t.kind = 'DIRECT'
              AND tm.user_id = $1
              AND t.direct_status = 'PENDING'
              AND t.requested_by IS NOT NULL
              AND t.requested_by <> $1
            ORDER BY t.created_at DESC
            "#,
        )
        .bind(receiver_user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(rows)
    }

    async fn find_direct_thread_for_member(
        &self,
        thread_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<Option<DirectThreadSummary>> {
        let row = sqlx::query_as::<_, DirectThreadSummary>(
            r#"
            SELECT
                t.id,
                CASE
                    WHEN t.direct_a_user_id = $2 THEN t.direct_b_user_id
                    ELSE t.direct_a_user_id
                END AS peer_user_id,
                u.username AS peer_username,
                t.direct_status AS status,
                t.requested_by,
                t.accepted_at,
                t.created_at,
                EXISTS(
                    SELECT 1
                    FROM thread_messages m
                    WHERE m.thread_id = t.id
                      AND m.deleted_at IS NULL
                      AND (
                        tr.last_read_message_id IS NULL
                        OR (m.created_at, m.id) > (
                            COALESCE(trm.created_at, '-infinity'::timestamptz),
                            COALESCE(trm.id, '00000000-0000-0000-0000-000000000000'::uuid)
                        )
                      )
                ) AS has_unread
            FROM threads t
            INNER JOIN thread_members tm
                ON tm.thread_id = t.id
            INNER JOIN users u
                ON u.id = CASE
                    WHEN t.direct_a_user_id = $2 THEN t.direct_b_user_id
                    ELSE t.direct_a_user_id
                END
            LEFT JOIN thread_reads tr
                ON tr.thread_id = t.id
               AND tr.user_id = $2
            LEFT JOIN thread_messages trm
                ON trm.id = tr.last_read_message_id
            WHERE t.kind = 'DIRECT'
              AND t.id = $1
              AND tm.user_id = $2
            LIMIT 1
            "#,
        )
        .bind(thread_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(row)
    }

    async fn accept_direct_thread_request(
        &self,
        thread_id: Uuid,
        receiver_user_id: Uuid,
    ) -> AppResult<Option<DirectThreadTransition>> {
        let mut tx = self.pool.begin().await.map_err(AppError::from)?;
        let thread = lock_direct_thread_for_member(&mut tx, thread_id, receiver_user_id).await?;
        let Some(thread) = thread else {
            return Ok(None);
        };

        if thread.direct_status != "PENDING" {
            return Err(AppError::validation("direct thread is already active"));
        }

        let requester_user_id = thread
            .requested_by
            .ok_or_else(|| AppError::internal("pending direct thread missing requester"))?;

        if requester_user_id == receiver_user_id {
            return Err(AppError::validation(
                "requester cannot accept own direct thread",
            ));
        }

        let accepted_at = sqlx::query_scalar::<_, OffsetDateTime>(
            r#"
            UPDATE threads
            SET direct_status = 'ACTIVE',
                accepted_at = NOW()
            WHERE id = $1
            RETURNING accepted_at
            "#,
        )
        .bind(thread.id)
        .fetch_one(tx.as_mut())
        .await
        .map_err(AppError::from)?;

        tx.commit().await.map_err(AppError::from)?;
        Ok(Some(DirectThreadTransition {
            thread_id,
            requester_user_id,
            receiver_user_id,
            accepted_at: Some(accepted_at),
        }))
    }

    async fn reject_direct_thread_request(
        &self,
        thread_id: Uuid,
        receiver_user_id: Uuid,
    ) -> AppResult<Option<DirectThreadTransition>> {
        let mut tx = self.pool.begin().await.map_err(AppError::from)?;
        let thread = lock_direct_thread_for_member(&mut tx, thread_id, receiver_user_id).await?;
        let Some(thread) = thread else {
            return Ok(None);
        };

        if thread.direct_status != "PENDING" {
            return Err(AppError::validation(
                "only pending direct thread can be rejected",
            ));
        }

        let requester_user_id = thread
            .requested_by
            .ok_or_else(|| AppError::internal("pending direct thread missing requester"))?;

        if requester_user_id == receiver_user_id {
            return Err(AppError::validation(
                "requester cannot reject own direct thread",
            ));
        }

        sqlx::query("DELETE FROM threads WHERE id = $1")
            .bind(thread.id)
            .execute(tx.as_mut())
            .await
            .map_err(AppError::from)?;

        tx.commit().await.map_err(AppError::from)?;
        Ok(Some(DirectThreadTransition {
            thread_id,
            requester_user_id,
            receiver_user_id,
            accepted_at: None,
        }))
    }

    async fn create_group_thread(
        &self,
        owner_id: Uuid,
        name: &str,
        member_user_ids: &[Uuid],
    ) -> AppResult<GroupThreadSummary> {
        let mut tx = self.pool.begin().await.map_err(AppError::from)?;

        let group = sqlx::query_as::<_, GroupThreadSummary>(
            r#"
            INSERT INTO threads (id, kind, name, owner_id, direct_status)
            VALUES ($1, 'GROUP', $2, $3, 'ACTIVE')
            RETURNING id, name, owner_id, created_at
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(name)
        .bind(owner_id)
        .fetch_one(tx.as_mut())
        .await
        .map_err(map_fk_as_not_found)?;

        sqlx::query(
            r#"
            INSERT INTO thread_members (thread_id, user_id)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(group.id)
        .bind(owner_id)
        .execute(tx.as_mut())
        .await
        .map_err(AppError::from)?;

        if !member_user_ids.is_empty() {
            sqlx::query(
                r#"
                INSERT INTO thread_members (thread_id, user_id)
                SELECT $1, member_id
                FROM UNNEST($2::uuid[]) AS member_id
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(group.id)
            .bind(member_user_ids)
            .execute(tx.as_mut())
            .await
            .map_err(map_fk_as_not_found)?;
        }

        tx.commit().await.map_err(AppError::from)?;
        Ok(group)
    }

    async fn list_group_threads(&self, user_id: Uuid) -> AppResult<Vec<GroupThreadSummary>> {
        let rows = sqlx::query_as::<_, GroupThreadSummary>(
            r#"
            SELECT
                t.id,
                t.name,
                t.owner_id,
                t.created_at,
                EXISTS(
                    SELECT 1
                    FROM thread_messages m
                    WHERE m.thread_id = t.id
                      AND m.deleted_at IS NULL
                      AND (
                        tr.last_read_message_id IS NULL
                        OR (m.created_at, m.id) > (
                            COALESCE(trm.created_at, '-infinity'::timestamptz),
                            COALESCE(trm.id, '00000000-0000-0000-0000-000000000000'::uuid)
                        )
                      )
                ) AS has_unread
            FROM threads t
            INNER JOIN thread_members tm
                ON tm.thread_id = t.id
            LEFT JOIN thread_reads tr
                ON tr.thread_id = t.id
               AND tr.user_id = $1
            LEFT JOIN thread_messages trm
                ON trm.id = tr.last_read_message_id
            WHERE t.kind = 'GROUP'
              AND tm.user_id = $1
            ORDER BY t.created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(rows)
    }

    async fn find_group_thread_for_member(
        &self,
        thread_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<Option<GroupThreadSummary>> {
        let row = sqlx::query_as::<_, GroupThreadSummary>(
            r#"
            SELECT
                t.id,
                t.name,
                t.owner_id,
                t.created_at,
                EXISTS(
                    SELECT 1
                    FROM thread_messages m
                    WHERE m.thread_id = t.id
                      AND m.deleted_at IS NULL
                      AND (
                        tr.last_read_message_id IS NULL
                        OR (m.created_at, m.id) > (
                            COALESCE(trm.created_at, '-infinity'::timestamptz),
                            COALESCE(trm.id, '00000000-0000-0000-0000-000000000000'::uuid)
                        )
                      )
                ) AS has_unread
            FROM threads t
            INNER JOIN thread_members tm
                ON tm.thread_id = t.id
            LEFT JOIN thread_reads tr
                ON tr.thread_id = t.id
               AND tr.user_id = $2
            LEFT JOIN thread_messages trm
                ON trm.id = tr.last_read_message_id
            WHERE t.kind = 'GROUP'
              AND t.id = $1
              AND tm.user_id = $2
            LIMIT 1
            "#,
        )
        .bind(thread_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(row)
    }

    async fn add_group_member(&self, thread_id: Uuid, user_id: Uuid) -> AppResult<()> {
        let is_group = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM threads WHERE id = $1 AND kind = 'GROUP'
            )
            "#,
        )
        .bind(thread_id)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::from)?;

        if !is_group {
            return Err(AppError::not_found("group thread not found"));
        }

        let result = sqlx::query(
            r#"
            INSERT INTO thread_members (thread_id, user_id)
            VALUES ($1, $2)
            "#,
        )
        .bind(thread_id)
        .bind(user_id)
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(err) => {
                if is_unique_violation(&err) {
                    return Err(AppError::conflict("user is already a group member"));
                }
                Err(map_fk_as_not_found(err))
            }
        }
    }

    async fn remove_group_member(&self, thread_id: Uuid, user_id: Uuid) -> AppResult<bool> {
        let result = sqlx::query(
            r#"
            DELETE FROM thread_members tm
            USING threads t
            WHERE tm.thread_id = t.id
              AND t.kind = 'GROUP'
              AND tm.thread_id = $1
              AND tm.user_id = $2
            "#,
        )
        .bind(thread_id)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(result.rows_affected() > 0)
    }

    async fn is_group_owner(&self, thread_id: Uuid, user_id: Uuid) -> AppResult<bool> {
        let owner = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM threads
                WHERE id = $1
                  AND kind = 'GROUP'
                  AND owner_id = $2
            )
            "#,
        )
        .bind(thread_id)
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(owner)
    }

    async fn is_thread_member(&self, thread_id: Uuid, user_id: Uuid) -> AppResult<bool> {
        let member = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM thread_members
                WHERE thread_id = $1 AND user_id = $2
            )
            "#,
        )
        .bind(thread_id)
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(member)
    }

    async fn find_thread_access(
        &self,
        thread_id: Uuid,
        user_id: Uuid,
    ) -> AppResult<Option<ThreadAccessRecord>> {
        let row = sqlx::query_as::<_, ThreadAccessRecord>(
            r#"
            SELECT
                t.id AS thread_id,
                t.kind,
                t.direct_status,
                t.requested_by
            FROM threads t
            INNER JOIN thread_members tm
                ON tm.thread_id = t.id
            WHERE t.id = $1
              AND tm.user_id = $2
            LIMIT 1
            "#,
        )
        .bind(thread_id)
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(row)
    }

    async fn create_thread_message(
        &self,
        thread_id: Uuid,
        author_id: Uuid,
        content: &str,
    ) -> AppResult<ThreadMessageRecord> {
        sqlx::query_as::<_, ThreadMessageRecord>(
            r#"
            INSERT INTO thread_messages (id, thread_id, author_id, content)
            VALUES ($1, $2, $3, $4)
            RETURNING id, thread_id, author_id, content, created_at, edited_at, deleted_at
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(thread_id)
        .bind(author_id)
        .bind(content)
        .fetch_one(&self.pool)
        .await
        .map_err(map_fk_as_not_found)
    }

    async fn list_thread_messages(
        &self,
        thread_id: Uuid,
        before_message_id: Option<Uuid>,
        limit: i64,
    ) -> AppResult<Vec<ThreadMessageRecord>> {
        let anchor = find_anchor(&self.pool, thread_id, before_message_id).await?;

        let rows = match anchor {
            Some((anchor_created_at, anchor_id)) => sqlx::query_as::<_, ThreadMessageRecord>(
                r#"
                    SELECT id, thread_id, author_id, content, created_at, edited_at, deleted_at
                    FROM thread_messages
                    WHERE thread_id = $1
                      AND deleted_at IS NULL
                      AND (created_at, id) < ($2, $3)
                    ORDER BY created_at DESC, id DESC
                    LIMIT $4
                    "#,
            )
            .bind(thread_id)
            .bind(anchor_created_at)
            .bind(anchor_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::from)?,
            None => sqlx::query_as::<_, ThreadMessageRecord>(
                r#"
                    SELECT id, thread_id, author_id, content, created_at, edited_at, deleted_at
                    FROM thread_messages
                    WHERE thread_id = $1
                      AND deleted_at IS NULL
                    ORDER BY created_at DESC, id DESC
                    LIMIT $2
                    "#,
            )
            .bind(thread_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::from)?,
        };

        Ok(rows)
    }

    async fn find_thread_message_by_id(
        &self,
        message_id: Uuid,
    ) -> AppResult<Option<ThreadMessageRecord>> {
        let row = sqlx::query_as::<_, ThreadMessageRecord>(
            r#"
            SELECT id, thread_id, author_id, content, created_at, edited_at, deleted_at
            FROM thread_messages
            WHERE id = $1
            LIMIT 1
            "#,
        )
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(row)
    }

    async fn update_thread_message_content(
        &self,
        message_id: Uuid,
        content: &str,
    ) -> AppResult<Option<ThreadMessageRecord>> {
        let row = sqlx::query_as::<_, ThreadMessageRecord>(
            r#"
            UPDATE thread_messages
            SET content = $2,
                edited_at = NOW()
            WHERE id = $1
              AND deleted_at IS NULL
            RETURNING id, thread_id, author_id, content, created_at, edited_at, deleted_at
            "#,
        )
        .bind(message_id)
        .bind(content)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(row)
    }

    async fn soft_delete_thread_message(
        &self,
        message_id: Uuid,
    ) -> AppResult<Option<ThreadMessageRecord>> {
        let row = sqlx::query_as::<_, ThreadMessageRecord>(
            r#"
            UPDATE thread_messages
            SET deleted_at = NOW()
            WHERE id = $1
              AND deleted_at IS NULL
            RETURNING id, thread_id, author_id, content, created_at, edited_at, deleted_at
            "#,
        )
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(row)
    }

    async fn create_group_invite(
        &self,
        thread_id: Uuid,
        created_by: Uuid,
        token: &str,
        expires_at: OffsetDateTime,
        max_uses: Option<i32>,
    ) -> AppResult<GroupInviteRecord> {
        let invite = sqlx::query_as::<_, GroupInviteRecord>(
            r#"
            INSERT INTO group_invites (token, thread_id, created_by, expires_at, max_uses)
            SELECT $1, $2, $3, $4, $5
            WHERE EXISTS (
                SELECT 1 FROM threads WHERE id = $2 AND kind = 'GROUP'
            )
            RETURNING token, thread_id, created_by, expires_at, max_uses, used_count, created_at
            "#,
        )
        .bind(token)
        .bind(thread_id)
        .bind(created_by)
        .bind(expires_at)
        .bind(max_uses)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_fk_as_not_found)?;

        invite.ok_or_else(|| AppError::not_found("group thread not found"))
    }

    async fn join_group_via_invite(
        &self,
        token: &str,
        user_id: Uuid,
    ) -> AppResult<GroupThreadSummary> {
        let mut tx = self.pool.begin().await.map_err(AppError::from)?;
        let invite = sqlx::query_as::<_, JoinInviteRow>(
            r#"
            SELECT
                gi.thread_id,
                t.name,
                t.owner_id,
                t.created_at AS thread_created_at,
                gi.expires_at,
                gi.max_uses,
                gi.used_count,
                gi.revoked_at
            FROM group_invites gi
            INNER JOIN threads t ON t.id = gi.thread_id
            WHERE gi.token = $1
              AND t.kind = 'GROUP'
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

        if invite.expires_at < OffsetDateTime::now_utc() {
            return Err(AppError::validation("invite token is expired"));
        }

        if let Some(max_uses) = invite.max_uses
            && invite.used_count >= max_uses
        {
            return Err(AppError::validation("invite token usage limit exceeded"));
        }

        let join_result = sqlx::query(
            r#"
            INSERT INTO thread_members (thread_id, user_id)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(invite.thread_id)
        .bind(user_id)
        .execute(tx.as_mut())
        .await
        .map_err(map_fk_as_not_found)?;

        if join_result.rows_affected() > 0 {
            sqlx::query(
                r#"
                UPDATE group_invites
                SET used_count = used_count + 1
                WHERE token = $1
                "#,
            )
            .bind(token)
            .execute(tx.as_mut())
            .await
            .map_err(AppError::from)?;
        }

        tx.commit().await.map_err(AppError::from)?;
        Ok(GroupThreadSummary {
            id: invite.thread_id,
            name: invite.name,
            owner_id: invite.owner_id,
            created_at: invite.thread_created_at,
            has_unread: false,
        })
    }

    async fn mark_thread_read(
        &self,
        thread_id: Uuid,
        user_id: Uuid,
        last_read_message_id: Option<Uuid>,
    ) -> AppResult<ThreadReadState> {
        let resolved_last_read =
            resolve_thread_last_read_message_id(&self.pool, thread_id, last_read_message_id)
                .await?;

        let read = sqlx::query_as::<_, ThreadReadState>(
            r#"
            INSERT INTO thread_reads (user_id, thread_id, last_read_message_id)
            VALUES ($1, $2, $3)
            ON CONFLICT (user_id, thread_id)
            DO UPDATE
            SET last_read_message_id = EXCLUDED.last_read_message_id,
                updated_at = NOW()
            RETURNING user_id, thread_id, last_read_message_id, updated_at
            "#,
        )
        .bind(user_id)
        .bind(thread_id)
        .bind(resolved_last_read)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(read)
    }
}

async fn ensure_direct_membership(
    tx: &mut Transaction<'_, Postgres>,
    thread_id: Uuid,
    a: Uuid,
    b: Uuid,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO thread_members (thread_id, user_id)
        VALUES ($1, $2), ($1, $3)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(thread_id)
    .bind(a)
    .bind(b)
    .execute(tx.as_mut())
    .await
    .map_err(AppError::from)?;

    Ok(())
}

async fn direct_summary_for_member(
    tx: &mut Transaction<'_, Postgres>,
    thread_id: Uuid,
    user_id: Uuid,
) -> AppResult<DirectThreadSummary> {
    sqlx::query_as::<_, DirectThreadSummary>(
        r#"
        SELECT
            t.id,
            CASE
                WHEN t.direct_a_user_id = $2 THEN t.direct_b_user_id
                ELSE t.direct_a_user_id
            END AS peer_user_id,
            u.username AS peer_username,
            t.direct_status AS status,
            t.requested_by,
            t.accepted_at,
            t.created_at,
            EXISTS(
                SELECT 1
                FROM thread_messages m
                WHERE m.thread_id = t.id
                  AND m.deleted_at IS NULL
                  AND (
                    tr.last_read_message_id IS NULL
                    OR (m.created_at, m.id) > (
                        COALESCE(trm.created_at, '-infinity'::timestamptz),
                        COALESCE(trm.id, '00000000-0000-0000-0000-000000000000'::uuid)
                    )
                  )
            ) AS has_unread
        FROM threads t
        INNER JOIN users u
            ON u.id = CASE
                WHEN t.direct_a_user_id = $2 THEN t.direct_b_user_id
                ELSE t.direct_a_user_id
            END
        LEFT JOIN thread_reads tr
            ON tr.thread_id = t.id
           AND tr.user_id = $2
        LEFT JOIN thread_messages trm
            ON trm.id = tr.last_read_message_id
        WHERE t.id = $1
          AND t.kind = 'DIRECT'
        LIMIT 1
        "#,
    )
    .bind(thread_id)
    .bind(user_id)
    .fetch_one(tx.as_mut())
    .await
    .map_err(AppError::from)
}

async fn lock_direct_thread_for_member(
    tx: &mut Transaction<'_, Postgres>,
    thread_id: Uuid,
    user_id: Uuid,
) -> AppResult<Option<DirectThreadStateRow>> {
    let row = sqlx::query_as::<_, (Uuid, String, Option<Uuid>)>(
        r#"
        SELECT
            t.id,
            t.direct_status,
            t.requested_by
        FROM threads t
        INNER JOIN thread_members tm
            ON tm.thread_id = t.id
        WHERE t.id = $1
          AND t.kind = 'DIRECT'
          AND tm.user_id = $2
        LIMIT 1
        FOR UPDATE
        "#,
    )
    .bind(thread_id)
    .bind(user_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(AppError::from)?;

    Ok(row.map(|v| DirectThreadStateRow {
        id: v.0,
        direct_status: v.1,
        requested_by: v.2,
    }))
}

async fn find_anchor(
    pool: &PgPool,
    thread_id: Uuid,
    before_message_id: Option<Uuid>,
) -> AppResult<Option<(OffsetDateTime, Uuid)>> {
    let Some(before_message_id) = before_message_id else {
        return Ok(None);
    };

    let row = sqlx::query_as::<_, (OffsetDateTime, Uuid)>(
        r#"
        SELECT created_at, id
        FROM thread_messages
        WHERE id = $1 AND thread_id = $2
        LIMIT 1
        "#,
    )
    .bind(before_message_id)
    .bind(thread_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::from)?;

    match row {
        Some(value) => Ok(Some(value)),
        None => Err(AppError::validation("invalid before cursor")),
    }
}

async fn resolve_thread_last_read_message_id(
    pool: &PgPool,
    thread_id: Uuid,
    requested_message_id: Option<Uuid>,
) -> AppResult<Option<Uuid>> {
    let Some(message_id) = requested_message_id else {
        return sqlx::query_scalar::<_, Option<Uuid>>(
            r#"
            SELECT id
            FROM thread_messages
            WHERE thread_id = $1
              AND deleted_at IS NULL
            ORDER BY created_at DESC, id DESC
            LIMIT 1
            "#,
        )
        .bind(thread_id)
        .fetch_one(pool)
        .await
        .map_err(AppError::from);
    };

    let valid_message_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT id
        FROM thread_messages
        WHERE id = $1
          AND thread_id = $2
          AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(message_id)
    .bind(thread_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::from)?;

    let valid_message_id =
        valid_message_id.ok_or_else(|| AppError::validation("invalid last_read_message_id"))?;
    Ok(Some(valid_message_id))
}

fn sort_pair(a: Uuid, b: Uuid) -> (Uuid, Uuid) {
    if a < b { (a, b) } else { (b, a) }
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    matches!(
        err,
        sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("23505")
    )
}

fn map_fk_as_not_found(err: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(db_err) = &err
        && db_err.code().as_deref() == Some("23503")
    {
        return AppError::not_found("referenced user or thread not found");
    }

    AppError::from(err)
}
