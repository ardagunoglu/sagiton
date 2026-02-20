use crate::application::ports::message::MessageRepository;
use crate::domain::message::{ChannelReadState, MessageRecord};
use crate::shared::error::{AppError, AppResult};
use async_trait::async_trait;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Clone)]
pub struct SqlxMessageRepository {
    pool: PgPool,
}

impl SqlxMessageRepository {
    /// Creates repository instance from an existing pool.
    ///
    /// # Parameters
    /// - `pool`: PostgreSQL pool shared by the application.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MessageRepository for SqlxMessageRepository {
    async fn create_message(
        &self,
        channel_id: Uuid,
        author_id: Uuid,
        content: &str,
    ) -> AppResult<MessageRecord> {
        sqlx::query_as::<_, MessageRecord>(
            r#"
            INSERT INTO messages (id, channel_id, author_id, content)
            VALUES ($1, $2, $3, $4)
            RETURNING id, channel_id, author_id, content, created_at, edited_at, deleted_at
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(channel_id)
        .bind(author_id)
        .bind(content)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::from)
    }

    async fn list_messages(
        &self,
        channel_id: Uuid,
        before_message_id: Option<Uuid>,
        limit: i64,
    ) -> AppResult<Vec<MessageRecord>> {
        let anchor = find_anchor(&self.pool, channel_id, before_message_id).await?;

        let rows = match anchor {
            Some((anchor_created_at, anchor_id)) => sqlx::query_as::<_, MessageRecord>(
                r#"
                    SELECT id, channel_id, author_id, content, created_at, edited_at, deleted_at
                    FROM messages
                    WHERE channel_id = $1
                      AND deleted_at IS NULL
                      AND (created_at, id) < ($2, $3)
                    ORDER BY created_at DESC, id DESC
                    LIMIT $4
                    "#,
            )
            .bind(channel_id)
            .bind(anchor_created_at)
            .bind(anchor_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::from)?,
            None => sqlx::query_as::<_, MessageRecord>(
                r#"
                    SELECT id, channel_id, author_id, content, created_at, edited_at, deleted_at
                    FROM messages
                    WHERE channel_id = $1
                      AND deleted_at IS NULL
                    ORDER BY created_at DESC, id DESC
                    LIMIT $2
                    "#,
            )
            .bind(channel_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(AppError::from)?,
        };

        Ok(rows)
    }

    async fn find_message_by_id(&self, message_id: Uuid) -> AppResult<Option<MessageRecord>> {
        let row = sqlx::query_as::<_, MessageRecord>(
            r#"
            SELECT id, channel_id, author_id, content, created_at, edited_at, deleted_at
            FROM messages
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

    async fn update_message_content(
        &self,
        message_id: Uuid,
        content: &str,
    ) -> AppResult<Option<MessageRecord>> {
        let row = sqlx::query_as::<_, MessageRecord>(
            r#"
            UPDATE messages
            SET content = $2,
                edited_at = NOW()
            WHERE id = $1
              AND deleted_at IS NULL
            RETURNING id, channel_id, author_id, content, created_at, edited_at, deleted_at
            "#,
        )
        .bind(message_id)
        .bind(content)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(row)
    }

    async fn soft_delete_message(&self, message_id: Uuid) -> AppResult<Option<MessageRecord>> {
        let row = sqlx::query_as::<_, MessageRecord>(
            r#"
            UPDATE messages
            SET deleted_at = NOW()
            WHERE id = $1
              AND deleted_at IS NULL
            RETURNING id, channel_id, author_id, content, created_at, edited_at, deleted_at
            "#,
        )
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(row)
    }

    async fn mark_channel_read(
        &self,
        channel_id: Uuid,
        user_id: Uuid,
        last_read_message_id: Option<Uuid>,
    ) -> AppResult<ChannelReadState> {
        let resolved_last_read =
            resolve_channel_last_read_message_id(&self.pool, channel_id, last_read_message_id)
                .await?;

        let read = sqlx::query_as::<_, ChannelReadState>(
            r#"
            INSERT INTO channel_reads (user_id, channel_id, last_read_message_id)
            VALUES ($1, $2, $3)
            ON CONFLICT (user_id, channel_id)
            DO UPDATE
            SET last_read_message_id = EXCLUDED.last_read_message_id,
                updated_at = NOW()
            RETURNING user_id, channel_id, last_read_message_id, updated_at
            "#,
        )
        .bind(user_id)
        .bind(channel_id)
        .bind(resolved_last_read)
        .fetch_one(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(read)
    }
}

async fn find_anchor(
    pool: &PgPool,
    channel_id: Uuid,
    before_message_id: Option<Uuid>,
) -> AppResult<Option<(OffsetDateTime, Uuid)>> {
    let Some(before_message_id) = before_message_id else {
        return Ok(None);
    };

    let row = sqlx::query_as::<_, (OffsetDateTime, Uuid)>(
        r#"
        SELECT created_at, id
        FROM messages
        WHERE id = $1 AND channel_id = $2
        LIMIT 1
        "#,
    )
    .bind(before_message_id)
    .bind(channel_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::from)?;

    match row {
        Some(value) => Ok(Some(value)),
        None => Err(AppError::validation("invalid before cursor")),
    }
}

async fn resolve_channel_last_read_message_id(
    pool: &PgPool,
    channel_id: Uuid,
    requested_message_id: Option<Uuid>,
) -> AppResult<Option<Uuid>> {
    let Some(message_id) = requested_message_id else {
        return sqlx::query_scalar::<_, Option<Uuid>>(
            r#"
            SELECT id
            FROM messages
            WHERE channel_id = $1
              AND deleted_at IS NULL
            ORDER BY created_at DESC, id DESC
            LIMIT 1
            "#,
        )
        .bind(channel_id)
        .fetch_one(pool)
        .await
        .map_err(AppError::from);
    };

    let valid_message_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT id
        FROM messages
        WHERE id = $1
          AND channel_id = $2
          AND deleted_at IS NULL
        LIMIT 1
        "#,
    )
    .bind(message_id)
    .bind(channel_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::from)?;

    let valid_message_id =
        valid_message_id.ok_or_else(|| AppError::validation("invalid last_read_message_id"))?;
    Ok(Some(valid_message_id))
}
