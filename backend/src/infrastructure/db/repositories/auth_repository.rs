use crate::application::ports::auth::{SessionRepository, UserRepository};
use crate::domain::auth::{SessionCreate, SessionRotate, UserProfile, UserRecord};
use crate::shared::error::{AppError, AppResult};
use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Clone)]
pub struct SqlxAuthRepository {
    pool: PgPool,
}

impl SqlxAuthRepository {
    /// Creates repository instance from an existing pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserRepository for SqlxAuthRepository {
    async fn create_user(
        &self,
        username: &str,
        email: Option<&str>,
        password_hash: &str,
    ) -> AppResult<UserProfile> {
        let user_id = Uuid::new_v4();

        let result = sqlx::query_as::<_, UserProfile>(
            r#"
            INSERT INTO users (id, username, email, password_hash)
            VALUES ($1, $2, $3, $4)
            RETURNING id, username, email
            "#,
        )
        .bind(user_id)
        .bind(username)
        .bind(email)
        .bind(password_hash)
        .fetch_one(&self.pool)
        .await;

        match result {
            Ok(user) => Ok(user),
            Err(err) => Err(map_create_user_error(err)),
        }
    }

    async fn find_user_by_login_identity(&self, identity: &str) -> AppResult<Option<UserRecord>> {
        let row = sqlx::query_as::<_, UserRecord>(
            r#"
            SELECT id, username, email, password_hash
            FROM users
            WHERE username = $1 OR email = $1
            LIMIT 1
            "#,
        )
        .bind(identity)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(row)
    }

    async fn find_user_profile_by_id(&self, user_id: Uuid) -> AppResult<Option<UserProfile>> {
        let row = sqlx::query_as::<_, UserProfile>(
            r#"
            SELECT id, username, email
            FROM users
            WHERE id = $1
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(row)
    }
}

#[async_trait]
impl SessionRepository for SqlxAuthRepository {
    async fn create_session(&self, session: SessionCreate) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO user_sessions (id, user_id, refresh_token_hash, expires_at, user_agent, ip)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(session.user_id)
        .bind(session.refresh_token_hash)
        .bind(session.expires_at)
        .bind(session.context.user_agent)
        .bind(session.context.ip)
        .execute(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(())
    }

    async fn rotate_refresh_session(&self, payload: SessionRotate) -> AppResult<Uuid> {
        let mut tx = self.pool.begin().await.map_err(AppError::from)?;

        let user_id = revoke_old_refresh_token(&mut tx, &payload.old_refresh_token_hash).await?;
        insert_rotated_session(
            &mut tx,
            user_id,
            &payload.new_refresh_token_hash,
            payload.expires_at,
            payload.context.user_agent,
            payload.context.ip,
        )
        .await?;

        tx.commit().await.map_err(AppError::from)?;
        Ok(user_id)
    }

    async fn revoke_by_refresh_hash(&self, refresh_token_hash: &str) -> AppResult<()> {
        sqlx::query(
            r#"
            UPDATE user_sessions
            SET revoked_at = NOW()
            WHERE refresh_token_hash = $1 AND revoked_at IS NULL
            "#,
        )
        .bind(refresh_token_hash)
        .execute(&self.pool)
        .await
        .map_err(AppError::from)?;

        Ok(())
    }
}

async fn revoke_old_refresh_token(
    tx: &mut Transaction<'_, Postgres>,
    refresh_token_hash: &str,
) -> AppResult<Uuid> {
    let maybe_user_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        UPDATE user_sessions
        SET revoked_at = NOW()
        WHERE refresh_token_hash = $1
          AND revoked_at IS NULL
          AND expires_at > NOW()
        RETURNING user_id
        "#,
    )
    .bind(refresh_token_hash)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(AppError::from)?;

    maybe_user_id.ok_or_else(|| AppError::unauthorized("invalid refresh token"))
}

async fn insert_rotated_session(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    refresh_token_hash: &str,
    expires_at: time::OffsetDateTime,
    user_agent: Option<String>,
    ip: Option<String>,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO user_sessions (id, user_id, refresh_token_hash, expires_at, user_agent, ip)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(refresh_token_hash)
    .bind(expires_at)
    .bind(user_agent)
    .bind(ip)
    .execute(tx.as_mut())
    .await
    .map_err(AppError::from)?;

    Ok(())
}

fn map_create_user_error(err: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(db_err) = &err
        && db_err.code().as_deref() == Some("23505")
    {
        return AppError::conflict("username or email already exists");
    }

    AppError::from(err)
}
