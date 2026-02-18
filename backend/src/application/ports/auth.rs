use crate::domain::auth::{SessionCreate, SessionRotate, UserProfile, UserRecord};
use crate::shared::error::AppResult;
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait UserRepository: Send + Sync {
    /// Persists a new user with a hashed password.
    async fn create_user(
        &self,
        username: &str,
        email: Option<&str>,
        password_hash: &str,
    ) -> AppResult<UserProfile>;

    /// Retrieves user record by username or email for login.
    async fn find_user_by_login_identity(&self, identity: &str) -> AppResult<Option<UserRecord>>;

    /// Retrieves public profile by user id.
    async fn find_user_profile_by_id(&self, user_id: Uuid) -> AppResult<Option<UserProfile>>;
}

#[async_trait]
pub trait SessionRepository: Send + Sync {
    /// Creates a new refresh session for the user.
    async fn create_session(&self, session: SessionCreate) -> AppResult<()>;

    /// Revokes old refresh token and inserts a new one atomically.
    async fn rotate_refresh_session(&self, payload: SessionRotate) -> AppResult<Uuid>;

    /// Revokes the session represented by refresh token hash.
    async fn revoke_by_refresh_hash(&self, refresh_token_hash: &str) -> AppResult<()>;
}

pub trait PasswordPort: Send + Sync {
    /// Hashes plaintext password using a memory-hard algorithm.
    fn hash_password(&self, password: &str) -> AppResult<String>;

    /// Verifies plaintext password against stored hash.
    fn verify_password(&self, password: &str, password_hash: &str) -> AppResult<bool>;
}

pub trait JwtPort: Send + Sync {
    /// Issues a short-lived access token for the provided user id.
    fn issue_access_token(&self, user_id: Uuid) -> AppResult<String>;

    /// Verifies access token signature and expiration, then returns user id.
    fn verify_access_token(&self, token: &str) -> AppResult<Uuid>;
}

pub trait RefreshTokenPort: Send + Sync {
    /// Generates a high-entropy refresh token string.
    fn generate_refresh_token(&self) -> String;

    /// Produces deterministic hash value for refresh token persistence.
    fn hash_refresh_token(&self, token: &str) -> String;
}
