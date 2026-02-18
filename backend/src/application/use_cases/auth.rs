use crate::application::ports::auth::{
    JwtPort, PasswordPort, RefreshTokenPort, SessionRepository, UserRepository,
};
use crate::domain::auth::{SessionContext, SessionCreate, SessionRotate, TokenPair, UserProfile};
use crate::shared::error::{AppError, AppResult};
use std::sync::Arc;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

const REFRESH_TOKEN_TTL_DAYS: i64 = 30;

#[derive(Clone)]
pub struct AuthUseCase {
    user_repo: Arc<dyn UserRepository>,
    session_repo: Arc<dyn SessionRepository>,
    password_port: Arc<dyn PasswordPort>,
    jwt_port: Arc<dyn JwtPort>,
    refresh_port: Arc<dyn RefreshTokenPort>,
}

pub struct RegisterCommand {
    pub username: String,
    pub email: Option<String>,
    pub password: String,
}

pub struct LoginCommand {
    pub username_or_email: String,
    pub password: String,
    pub context: SessionContext,
}

pub struct RefreshCommand {
    pub refresh_token: String,
    pub context: SessionContext,
}

pub struct LogoutCommand {
    pub refresh_token: String,
}

impl AuthUseCase {
    /// Builds auth use-case with repository and crypto adapters.
    pub fn new(
        user_repo: Arc<dyn UserRepository>,
        session_repo: Arc<dyn SessionRepository>,
        password_port: Arc<dyn PasswordPort>,
        jwt_port: Arc<dyn JwtPort>,
        refresh_port: Arc<dyn RefreshTokenPort>,
    ) -> Self {
        Self {
            user_repo,
            session_repo,
            password_port,
            jwt_port,
            refresh_port,
        }
    }

    /// Registers a new user with hashed password.
    pub async fn register(&self, cmd: RegisterCommand) -> AppResult<UserProfile> {
        validate_username(&cmd.username)?;
        validate_password(&cmd.password)?;
        let email = normalize_optional_email(cmd.email)?;

        let password_hash = self.password_port.hash_password(cmd.password.trim())?;
        self.user_repo
            .create_user(cmd.username.trim(), email.as_deref(), &password_hash)
            .await
    }

    /// Logs in user and creates refresh session.
    pub async fn login(&self, cmd: LoginCommand) -> AppResult<TokenPair> {
        let identity = cmd.username_or_email.trim();
        if identity.is_empty() {
            return Err(AppError::validation("username_or_email is required"));
        }

        let user = self
            .user_repo
            .find_user_by_login_identity(identity)
            .await?
            .ok_or_else(|| AppError::unauthorized("invalid credentials"))?;

        let password_ok = self
            .password_port
            .verify_password(cmd.password.trim(), &user.password_hash)?;

        if !password_ok {
            return Err(AppError::unauthorized("invalid credentials"));
        }

        self.issue_and_persist_tokens(user.id, cmd.context).await
    }

    /// Rotates refresh token and issues a fresh access token.
    pub async fn refresh(&self, cmd: RefreshCommand) -> AppResult<TokenPair> {
        let refresh_token = cmd.refresh_token.trim();
        if refresh_token.is_empty() {
            return Err(AppError::validation("refresh_token is required"));
        }

        let old_hash = self.refresh_port.hash_refresh_token(refresh_token);
        let new_refresh_token = self.refresh_port.generate_refresh_token();
        let new_hash = self.refresh_port.hash_refresh_token(&new_refresh_token);

        let expires_at = build_refresh_expiry();
        let user_id = self
            .session_repo
            .rotate_refresh_session(SessionRotate {
                old_refresh_token_hash: old_hash,
                new_refresh_token_hash: new_hash,
                expires_at,
                context: cmd.context,
            })
            .await?;

        let access_token = self.jwt_port.issue_access_token(user_id)?;
        Ok(TokenPair {
            access_token,
            refresh_token: new_refresh_token,
        })
    }

    /// Revokes active session represented by refresh token.
    pub async fn logout(&self, cmd: LogoutCommand) -> AppResult<()> {
        let refresh_token = cmd.refresh_token.trim();
        if refresh_token.is_empty() {
            return Err(AppError::validation("refresh_token is required"));
        }

        let refresh_hash = self.refresh_port.hash_refresh_token(refresh_token);
        self.session_repo
            .revoke_by_refresh_hash(&refresh_hash)
            .await
    }

    /// Retrieves current user profile for authorized user id.
    pub async fn me(&self, user_id: Uuid) -> AppResult<UserProfile> {
        self.user_repo
            .find_user_profile_by_id(user_id)
            .await?
            .ok_or_else(|| AppError::not_found("user not found"))
    }

    /// Validates and decodes access token from bearer header.
    pub fn verify_access_token(&self, token: &str) -> AppResult<Uuid> {
        self.jwt_port.verify_access_token(token)
    }

    async fn issue_and_persist_tokens(
        &self,
        user_id: Uuid,
        context: SessionContext,
    ) -> AppResult<TokenPair> {
        let access_token = self.jwt_port.issue_access_token(user_id)?;
        let refresh_token = self.refresh_port.generate_refresh_token();
        let refresh_hash = self.refresh_port.hash_refresh_token(&refresh_token);

        self.session_repo
            .create_session(SessionCreate {
                user_id,
                refresh_token_hash: refresh_hash,
                expires_at: build_refresh_expiry(),
                context,
            })
            .await?;

        Ok(TokenPair {
            access_token,
            refresh_token,
        })
    }
}

fn validate_username(username: &str) -> AppResult<()> {
    let value = username.trim();
    if value.len() < 3 {
        return Err(AppError::validation(
            "username must be at least 3 characters",
        ));
    }
    if value.len() > 32 {
        return Err(AppError::validation(
            "username must be at most 32 characters",
        ));
    }
    Ok(())
}

fn validate_password(password: &str) -> AppResult<()> {
    let value = password.trim();
    if value.len() < 8 {
        return Err(AppError::validation(
            "password must be at least 8 characters",
        ));
    }
    Ok(())
}

fn normalize_optional_email(email: Option<String>) -> AppResult<Option<String>> {
    match email {
        Some(raw) => {
            let normalized = raw.trim().to_lowercase();
            if normalized.is_empty() {
                return Ok(None);
            }
            if !normalized.contains('@') {
                return Err(AppError::validation("email format is invalid"));
            }
            Ok(Some(normalized))
        }
        None => Ok(None),
    }
}

fn build_refresh_expiry() -> OffsetDateTime {
    OffsetDateTime::now_utc() + Duration::days(REFRESH_TOKEN_TTL_DAYS)
}
