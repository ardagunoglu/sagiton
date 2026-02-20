use crate::application::ports::friendship::FriendshipRepository;
use crate::domain::friendship::{FriendRequestRecord, FriendRequestSummary, FriendSummary};
use crate::shared::error::{AppError, AppResult};
use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Clone)]
pub struct SqlxFriendshipRepository {
    pool: PgPool,
}

impl SqlxFriendshipRepository {
    /// Creates repository instance from an existing pool.
    ///
    /// # Parameters
    /// - `pool`: PostgreSQL pool shared by the application.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FriendshipRepository for SqlxFriendshipRepository {
    async fn create_friend_request(
        &self,
        from_user_id: Uuid,
        to_user_id: Uuid,
    ) -> AppResult<FriendRequestRecord> {
        let (a, b) = sort_pair(from_user_id, to_user_id);

        let already_friends = sqlx::query_scalar::<_, bool>(
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
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::from)?;

        if already_friends {
            return Err(AppError::conflict("users are already friends"));
        }

        let incoming_pending_exists = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM friend_requests
                WHERE from_user_id = $1
                  AND to_user_id = $2
                  AND status = 'PENDING'
            )
            "#,
        )
        .bind(to_user_id)
        .bind(from_user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::from)?;

        if incoming_pending_exists {
            return Err(AppError::conflict(
                "incoming friend request already exists from this user",
            ));
        }

        let request = sqlx::query_as::<_, FriendRequestRecord>(
            r#"
            INSERT INTO friend_requests (from_user_id, to_user_id, status, created_at, responded_at)
            VALUES ($1, $2, 'PENDING', NOW(), NULL)
            ON CONFLICT (from_user_id, to_user_id)
            DO UPDATE
            SET status = 'PENDING',
                created_at = NOW(),
                responded_at = NULL
            WHERE friend_requests.status <> 'PENDING'
            RETURNING from_user_id, to_user_id, status, created_at, responded_at
            "#,
        )
        .bind(from_user_id)
        .bind(to_user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_fk_as_not_found)?;

        request.ok_or_else(|| AppError::conflict("friend request is already pending"))
    }

    async fn accept_friend_request(
        &self,
        from_user_id: Uuid,
        to_user_id: Uuid,
    ) -> AppResult<Option<FriendSummary>> {
        let mut tx = self.pool.begin().await.map_err(AppError::from)?;

        let status = sqlx::query_scalar::<_, String>(
            r#"
            SELECT status
            FROM friend_requests
            WHERE from_user_id = $1
              AND to_user_id = $2
            LIMIT 1
            FOR UPDATE
            "#,
        )
        .bind(from_user_id)
        .bind(to_user_id)
        .fetch_optional(tx.as_mut())
        .await
        .map_err(AppError::from)?;

        let Some(status) = status else {
            return Ok(None);
        };

        if status == "REJECTED" {
            return Err(AppError::validation("friend request is already rejected"));
        }

        if status == "PENDING" {
            sqlx::query(
                r#"
                UPDATE friend_requests
                SET status = 'ACCEPTED',
                    responded_at = NOW()
                WHERE from_user_id = $1
                  AND to_user_id = $2
                "#,
            )
            .bind(from_user_id)
            .bind(to_user_id)
            .execute(tx.as_mut())
            .await
            .map_err(AppError::from)?;
        }

        let (a, b) = sort_pair(from_user_id, to_user_id);
        sqlx::query(
            r#"
            INSERT INTO friends (user_id_a, user_id_b)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(a)
        .bind(b)
        .execute(tx.as_mut())
        .await
        .map_err(AppError::from)?;

        sqlx::query(
            r#"
            UPDATE threads
            SET direct_status = 'ACTIVE',
                accepted_at = COALESCE(accepted_at, NOW())
            WHERE kind = 'DIRECT'
              AND direct_a_user_id = $1
              AND direct_b_user_id = $2
              AND direct_status = 'PENDING'
            "#,
        )
        .bind(a)
        .bind(b)
        .execute(tx.as_mut())
        .await
        .map_err(AppError::from)?;

        let friend = friend_summary_for_user(&mut tx, to_user_id, from_user_id).await?;

        tx.commit().await.map_err(AppError::from)?;
        Ok(Some(friend))
    }

    async fn reject_friend_request(&self, from_user_id: Uuid, to_user_id: Uuid) -> AppResult<bool> {
        let result = sqlx::query(
            r#"
            UPDATE friend_requests
            SET status = 'REJECTED',
                responded_at = NOW()
            WHERE from_user_id = $1
              AND to_user_id = $2
              AND status = 'PENDING'
            "#,
        )
        .bind(from_user_id)
        .bind(to_user_id)
        .execute(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(result.rows_affected() > 0)
    }

    async fn list_pending_inbox_requests(
        &self,
        user_id: Uuid,
    ) -> AppResult<Vec<FriendRequestSummary>> {
        let rows = sqlx::query_as::<_, FriendRequestSummary>(
            r#"
            SELECT
                fr.from_user_id,
                fr.to_user_id,
                fu.username AS from_username,
                tu.username AS to_username,
                fr.status,
                fr.created_at,
                fr.responded_at
            FROM friend_requests fr
            INNER JOIN users fu ON fu.id = fr.from_user_id
            INNER JOIN users tu ON tu.id = fr.to_user_id
            WHERE fr.to_user_id = $1
              AND fr.status = 'PENDING'
            ORDER BY fr.created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(rows)
    }

    async fn list_pending_outbox_requests(
        &self,
        user_id: Uuid,
    ) -> AppResult<Vec<FriendRequestSummary>> {
        let rows = sqlx::query_as::<_, FriendRequestSummary>(
            r#"
            SELECT
                fr.from_user_id,
                fr.to_user_id,
                fu.username AS from_username,
                tu.username AS to_username,
                fr.status,
                fr.created_at,
                fr.responded_at
            FROM friend_requests fr
            INNER JOIN users fu ON fu.id = fr.from_user_id
            INNER JOIN users tu ON tu.id = fr.to_user_id
            WHERE fr.from_user_id = $1
              AND fr.status = 'PENDING'
            ORDER BY fr.created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(rows)
    }

    async fn list_friends(&self, user_id: Uuid) -> AppResult<Vec<FriendSummary>> {
        let rows = sqlx::query_as::<_, FriendSummary>(
            r#"
            SELECT
                CASE
                    WHEN f.user_id_a = $1 THEN f.user_id_b
                    ELSE f.user_id_a
                END AS user_id,
                u.username,
                f.created_at
            FROM friends f
            INNER JOIN users u
                ON u.id = CASE
                    WHEN f.user_id_a = $1 THEN f.user_id_b
                    ELSE f.user_id_a
                END
            WHERE f.user_id_a = $1
               OR f.user_id_b = $1
            ORDER BY f.created_at DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(rows)
    }
}

async fn friend_summary_for_user(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    friend_user_id: Uuid,
) -> AppResult<FriendSummary> {
    let (a, b) = sort_pair(user_id, friend_user_id);

    sqlx::query_as::<_, FriendSummary>(
        r#"
        SELECT
            u.id AS user_id,
            u.username,
            f.created_at
        FROM friends f
        INNER JOIN users u ON u.id = $3
        WHERE f.user_id_a = $1
          AND f.user_id_b = $2
        LIMIT 1
        "#,
    )
    .bind(a)
    .bind(b)
    .bind(friend_user_id)
    .fetch_one(tx.as_mut())
    .await
    .map_err(AppError::from)
}

fn sort_pair(a: Uuid, b: Uuid) -> (Uuid, Uuid) {
    if a < b { (a, b) } else { (b, a) }
}

fn map_fk_as_not_found(err: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(db_err) = &err
        && db_err.code().as_deref() == Some("23503")
    {
        return AppError::not_found("referenced user not found");
    }

    AppError::from(err)
}
